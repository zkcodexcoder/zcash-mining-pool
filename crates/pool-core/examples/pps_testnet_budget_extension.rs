//! Explicit owner-only allowance extension. Never called by runtime startup.
//! --preflight opens the existing DB read-only and verifies projected backing.
//! --extend freshly repeats that proof and calls the one-time atomic DB API.
//! Credentials/config enter only via a bounded anonymous pipe; output has no
//! values or errors. After any uncertain mutation outcome, inspect the journal;
//! do not blindly retry or restore the predecessor accounting state. The owner
//! must stop both runtime writers and hold the DB path/parent directories
//! stable; the metadata check is not a kernel-enforced NOFOLLOW SQLite open.
use node_rpc::ZcashRpcClient;
use pool_core::pps_funding::{collect_pps_testnet_budget_extension_funding, PpsFundingError};
use pool_db::{PoolDb, pps_policy::PpsPolicy};
use serde::{Deserialize, Serialize};
use sha2::{Digest,Sha256};
use sqlx::ConnectOptions;
use std::{fs::File, io::Read, net::IpAddr, os::fd::{AsRawFd,FromRawFd,RawFd},
    os::unix::fs::FileTypeExt, path::Path, process::ExitCode, time::{Duration,Instant}};

const MAX_INPUT_BYTES:u64=262_144;
const INPUT_LIMIT:Duration=Duration::from_secs(5);
const TOTAL_LIMIT:Duration=Duration::from_secs(60);
#[derive(Clone,Copy,PartialEq,Eq)]
enum Mode { Preflight, Extend }
fn mode(args: &[String])->Option<Mode> {
    match args { [arg] if arg=="--preflight"=>Some(Mode::Preflight),
        [arg] if arg=="--extend"=>Some(Mode::Extend), _=>None }
}
// Deliberately no Debug or Serialize on credential-bearing input.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input { database_path:String, previous_policy:PpsPolicy,
    hold_new_legacy_sends:bool, wallet:Wallet, node:Node }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wallet { rpc_url:String,rpc_user:Option<String>,rpc_password:Option<String>,source:String }
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node { rpc_url:String,rpc_user:Option<String>,rpc_password:Option<String>,rpc_url_sha256:String }
#[derive(Serialize)]
struct Report { input_valid:bool, read_only:bool, fully_backed:bool,
    extension_attempted:bool, commit_confirmed:bool, passed:bool, error_category:&'static str }
impl Report {
    fn new()->Self { Self {input_valid:false,read_only:true,fully_backed:false,
        extension_attempted:false,commit_confirmed:false,passed:false,error_category:"input_invalid"} }
}
fn endpoint(raw:&str)->Option<reqwest::Url> {
    if raw.len()>4096 {return None}
    let url=reqwest::Url::parse(raw).ok()?;
    (matches!(url.scheme(),"http"|"https") && url.host_str().is_some()
        && url.username().is_empty() && url.password().is_none()
        && url.query().is_none() && url.fragment().is_none()).then_some(url)
}
fn auth(user:&Option<String>,password:&Option<String>)->bool {
    match (user,password) {(None,None)=>true,(Some(u),Some(p))=>
        !u.is_empty() && u.len()<=8192 && p.len()<=8192 && !u.contains('\0') && !p.contains('\0'), _=>false}
}
fn parse_input(raw:&str)->Option<Input> {
    if raw.is_empty() || raw.len()>MAX_INPUT_BYTES as usize {return None}
    let input:Input=serde_json::from_str(raw).ok()?;
    let wallet=endpoint(&input.wallet.rpc_url)?;
    endpoint(&input.node.rpc_url)?;
    if !wallet.host_str()?.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>().ok()?.is_loopback()
        || !Path::new(&input.database_path).is_absolute() || input.database_path.len()>4096
        || input.database_path.contains('\0') || !input.hold_new_legacy_sends
        || input.wallet.source.is_empty() || input.wallet.source.len()>2048
        || !input.wallet.source.bytes().all(|b|b.is_ascii_graphic())
        || !auth(&input.wallet.rpc_user,&input.wallet.rpc_password)
        || !auth(&input.node.rpc_user,&input.node.rpc_password)
        || input.node.rpc_url_sha256.len()!=64
        || !input.node.rpc_url_sha256.eq_ignore_ascii_case(&format!("{:x}",Sha256::digest(input.node.rpc_url.as_bytes())))
    {return None}
    input.previous_policy.validate("testnet").ok()?;
    pool_db::pps_funding::testnet_budget_extension_epoch(&input.previous_policy.epoch_config()).ok()?;
    Some(input)
}
fn client(url:&str,user:&Option<String>,password:&Option<String>)->Result<ZcashRpcClient,()> {
    let transport=reqwest::Client::builder().no_proxy().redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(15)).build().map_err(|_|())?;
    Ok(ZcashRpcClient::with_transport(url,user.as_ref().zip(password.as_ref()).map(|(u,p)|(u.clone(),p.clone())),transport))
}
fn funding_category(error:PpsFundingError)->&'static str {
    match error { PpsFundingError::RoutePolicy=>"policy_mismatch",
        PpsFundingError::AccountingUnavailable=>"accounting_unavailable",
        PpsFundingError::Timeout=>"deadline_exceeded",
        PpsFundingError::InsufficientFunding=>"funding_insufficient",
        PpsFundingError::IdentitySignerNotProven=>"signer_unverified",
        PpsFundingError::ConcurrentChange=>"concurrent_change",
        _=>"funding_unverified" }
}
async fn execute(input:&Input,mode:Mode,report:&mut Report) {
    report.input_valid=true;
    // Reject a currently visible symlink, never create a DB or run general
    // migrations. Stable owner-controlled path is an explicit precondition.
    if !std::fs::symlink_metadata(&input.database_path).ok().is_some_and(|m|m.file_type().is_file()) {
        report.error_category="database_unavailable"; return;
    }
    let options=sqlx::sqlite::SqliteConnectOptions::new().filename(&input.database_path)
        .create_if_missing(false).read_only(mode==Mode::Preflight)
        .busy_timeout(Duration::from_secs(2)).disable_statement_logging();
    let pool=match sqlx::sqlite::SqlitePoolOptions::new().max_connections(1)
        .acquire_timeout(Duration::from_secs(3)).connect_with(options).await {
        Ok(pool)=>pool,Err(_)=>{report.error_category="database_unavailable";return}
    };
    let db=PoolDb::new(pool);
    let (wallet,node)=match (client(&input.wallet.rpc_url,&input.wallet.rpc_user,&input.wallet.rpc_password),
        client(&input.node.rpc_url,&input.node.rpc_user,&input.node.rpc_password)) {
        (Ok(wallet),Ok(node))=>(wallet,node),_=>{report.error_category="transport_unavailable";return}
    };
    let previous=input.previous_policy.epoch_config();
    let lease=match collect_pps_testnet_budget_extension_funding(&db,&wallet,&previous,&input.wallet.source,&node).await {
        Ok(lease)=>lease,Err(error)=>{report.error_category=funding_category(error);return}
    };
    report.fully_backed=true;
    if mode==Mode::Extend {
        report.read_only=false; report.extension_attempted=true;
        // Error/cancellation here can be an ambiguous commit. Fixed output
        // deliberately does not claim that an unconfirmed commit was absent.
        if db.extend_testnet_pps_budget(&previous,&lease).await.is_err() {
            report.error_category="extension_outcome_requires_inspection";return;
        }
        report.commit_confirmed=true;
    }
    report.passed=true;report.error_category="ok";
}

#[cfg(target_os = "linux")]
fn kernel_anonymous_pipe(fd: RawFd) -> bool {
    let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
    // SAFETY: stat points to writable correctly sized storage, initialized
    // by fstatfs only on success. PIPEFS_MAGIC identifies anonymous pipes.
    unsafe {
        libc::fstatfs(fd, stat.as_mut_ptr()) == 0 && stat.assume_init().f_type as u64 == 0x5049_5045
    }
}
#[cfg(target_os = "macos")]
fn kernel_anonymous_pipe(fd: RawFd) -> bool {
    // Darwin distinguishes pipe descriptors from named-FIFO vnode descriptors.
    // This fixed-size metadata-only inventory fails closed if truncated.
    let mut entries = [libc::proc_fdinfo {
        proc_fd: -1,
        proc_fdtype: 0,
    }; 256];
    let size = std::mem::size_of_val(&entries);
    // SAFETY: the fixed array is writable for exactly size bytes. We request
    // only this process's FD numbers/types, not paths or input contents.
    let received = unsafe {
        libc::proc_pidinfo(
            libc::getpid(),
            libc::PROC_PIDLISTFDS,
            0,
            entries.as_mut_ptr().cast(),
            size as libc::c_int,
        )
    };
    if received <= 0
        || received as usize >= size
        || received as usize % std::mem::size_of::<libc::proc_fdinfo>() != 0
    {
        return false;
    }
    entries[..received as usize / std::mem::size_of::<libc::proc_fdinfo>()]
        .iter()
        .any(|entry| entry.proc_fd == fd && entry.proc_fdtype == 6)
}
#[cfg(not(any(target_os = "linux", target_os = "macos")))]
fn kernel_anonymous_pipe(_fd: RawFd) -> bool {
    false
}

struct NonblockingPipe {
    file: File,
    original_flags: libc::c_int,
}
impl Drop for NonblockingPipe {
    fn drop(&mut self) {
        // SAFETY: descriptor remains owned/open until this Drop finishes.
        // Restore flags because dup shares the original open-file description.
        unsafe {
            libc::fcntl(self.file.as_raw_fd(), libc::F_SETFL, self.original_flags);
        }
    }
}

fn read_pipe_config(fd: RawFd, limit: Duration) -> Option<String> {
    // Duplicate only the supplied descriptor, never open /dev/stdin (which
    // could resolve a filesystem object instead of the inherited pipe).
    // SAFETY: fcntl returns a new owned descriptor or a negative error.
    let copied = unsafe { libc::fcntl(fd, libc::F_DUPFD_CLOEXEC, 3) };
    if copied < 0 {
        return None;
    }
    // SAFETY: copied is newly owned and is closed exactly once by File.
    let file = unsafe { File::from_raw_fd(copied) };
    if !file.metadata().ok()?.file_type().is_fifo()
        || unsafe { libc::isatty(copied) } != 0
        || !kernel_anonymous_pipe(copied)
    {
        return None;
    }
    // Nonblocking reads make the deadline effective even if another inherited
    // reader consumes data between poll and read. Errors never leave this API.
    let flags = unsafe { libc::fcntl(copied, libc::F_GETFL) };
    if flags < 0
        || flags & libc::O_ACCMODE != libc::O_RDONLY
        || unsafe { libc::fcntl(copied, libc::F_SETFL, flags | libc::O_NONBLOCK) } != 0
    {
        return None;
    }
    let mut pipe = NonblockingPipe {
        file,
        original_flags: flags,
    };
    let deadline = std::time::Instant::now().checked_add(limit)?;
    let mut bytes = Vec::new();
    loop {
        let remaining = deadline.checked_duration_since(std::time::Instant::now())?;
        let timeout = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        let mut event = libc::pollfd {
            fd: copied,
            events: libc::POLLIN,
            revents: 0,
        };
        // SAFETY: event is one valid writable pollfd, and timeout is bounded.
        let polled = unsafe { libc::poll(&mut event, 1, timeout) };
        if polled <= 0 || event.revents & (libc::POLLERR | libc::POLLNVAL) != 0 {
            return None;
        }
        if bytes.len() == MAX_INPUT_BYTES as usize {
            // Confirm EOF without consuming a MAX+1 byte: input reads never
            // exceed the promised one-MiB bound, including oversize rejection.
            let mut available: libc::c_int = -1;
            if event.revents & libc::POLLHUP == 0
                || unsafe { libc::ioctl(copied, libc::FIONREAD, &mut available) } != 0
                || available != 0
            {
                return None;
            }
            return String::from_utf8(bytes).ok();
        }
        let mut buffer = [0_u8; 4096];
        let capacity = buffer.len().min(MAX_INPUT_BYTES as usize - bytes.len());
        match pipe.file.read(&mut buffer[..capacity]) {
            Ok(0) => {
                return if bytes.is_empty() {
                    None
                } else {
                    String::from_utf8(bytes).ok()
                }
            }
            Ok(count) => bytes.extend_from_slice(&buffer[..count]),
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock
                    || error.kind() == std::io::ErrorKind::Interrupted => {}
            Err(_) => return None,
        }
    }
}



#[tokio::main]
async fn main()->ExitCode {
    std::panic::set_hook(Box::new(|_|{}));
    let started=Instant::now();
    let mut report=Report::new();
    let args:Vec<String>=std::env::args().skip(1).take(3).collect();
    if let Some(mode)=mode(&args) {
        if let Some(input)=read_pipe_config(libc::STDIN_FILENO,INPUT_LIMIT).as_deref().and_then(parse_input) {
            if let Some(remaining)=TOTAL_LIMIT.checked_sub(started.elapsed()) {
                if tokio::time::timeout(remaining,execute(&input,mode,&mut report)).await.is_err() {
                    report.passed=false;
                    report.error_category=if report.extension_attempted {"extension_outcome_requires_inspection"} else {"deadline_exceeded"};
                }
            }
        }
    }
    println!("{}",serde_json::to_string(&report).unwrap_or_else(|_|String::from(
        "{\"input_valid\":false,\"read_only\":false,\"fully_backed\":false,\"extension_attempted\":false,\"commit_confirmed\":false,\"passed\":false,\"error_category\":\"serialization_failed\"}")));
    if report.passed {ExitCode::SUCCESS} else {ExitCode::from(1)}
}
#[cfg(test)]
mod tests {
    use super::*;
    fn input()->serde_json::Value {
        let node="http://127.0.0.1:18232";
        serde_json::json!({"database_path":"/private/tmp/synthetic-budget.sqlite",
            "previous_policy":{"network":"testnet","epoch":"synthetic-previous","fee_bps":100,
                "max_liability_zatoshis":950000000,"total_exposure_zatoshis":1000000000,
                "fee_allowance_zatoshis":50000000,"reserve_min_zatoshis":10,"max_payout_zatoshis":100},
            "hold_new_legacy_sends":true,"wallet":{"rpc_url":"http://127.0.0.1:28232","source":"synthetic-source"},
            "node":{"rpc_url":node,"rpc_url_sha256":format!("{:x}",Sha256::digest(node.as_bytes()))}})
    }
    #[test]
    fn exact_explicit_modes_only() {
        assert!(mode(&[]).is_none()); assert!(mode(&["--preflight".into()])==Some(Mode::Preflight));
        assert!(mode(&["--extend".into()])==Some(Mode::Extend));
        assert!(mode(&["--extend".into(),"--preflight".into()]).is_none());
        assert!(mode(&["--auto".into()]).is_none());
    }
    #[test]
    fn pure_input_rejects_policy_or_endpoint_drift() {
        assert!(parse_input(&input().to_string()).is_some());
        for variant in 0..7 {
            let mut v=input();
            match variant {0=>v["previous_policy"]["max_liability_zatoshis"]=serde_json::json!(95000000000_i64),
                1=>v["previous_policy"]["network"]=serde_json::json!("mainnet"),
                2=>v["hold_new_legacy_sends"]=serde_json::json!(false),
                3=>v["wallet"]["rpc_url"]=serde_json::json!("http://192.0.2.1:28232"),
                4=>v["node"]["rpc_url_sha256"]=serde_json::json!("0".repeat(64)),
                5=>v["database_path"]=serde_json::json!("relative.sqlite"),
                _=>v["unrecognized"]=serde_json::json!("never log this")}
            assert!(parse_input(&v.to_string()).is_none());
        }
    }
    #[test]
    fn fixed_report_contains_no_input_or_error_text() {
        let report=serde_json::to_value(Report::new()).unwrap();
        for (key,value) in report.as_object().unwrap() {
            assert!(value.is_boolean() || (key=="error_category" && value=="input_invalid"));
        }
        assert_eq!(funding_category(PpsFundingError::InsufficientFunding),"funding_insufficient");
        assert_eq!(funding_category(PpsFundingError::AccountingUnavailable),"accounting_unavailable");
    }
    #[test]
    fn regular_file_stdin_is_rejected() {
        let file=File::open("/dev/null").unwrap();
        assert!(read_pipe_config(file.as_raw_fd(),Duration::from_millis(5)).is_none());
    }
}
