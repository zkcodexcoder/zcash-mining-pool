//! Owner-run, read-only testnet wallet admission probe. No arguments are accepted.
//! Input is bounded JSON through an anonymous stdin pipe from a trusted wrapper.
//! This creates no ledger lease or liability and performs no send/key operation.
//! The wrapper must derive the required amount from current protected claims,
//! the full finite trial allocation, and the configured reserve. Values stay private.
use node_rpc::{ZcashRpcClient, zecd_funding::{collect_testnet_funding, ZecdFundingError}};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{fs::File, io::Read, net::IpAddr, os::fd::{AsRawFd, FromRawFd, RawFd},
    os::unix::fs::FileTypeExt, process::ExitCode, time::{Duration, Instant}};

const MAX_INPUT_BYTES: u64 = 262_144;
const INPUT_LIMIT: Duration = Duration::from_secs(5);
const TOTAL_LIMIT: Duration = Duration::from_secs(55);

// Deliberately no Debug or Serialize on credential-bearing inputs.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    network: String,
    wallet: Wallet,
    node: Node,
    required_spendable_zatoshis: i64,
    recipients: Vec<String>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wallet {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    source: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Node {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
    rpc_url_sha256: String,
}
#[derive(Serialize)]
struct Report {
    input_valid: bool,
    funding_verified: bool,
    signer_verified: bool,
    recipients_supported: bool,
    fully_backed: bool,
    read_only: bool,
    passed: bool,
    error_category: &'static str,
}
impl Report {
    fn new() -> Self {
        Self { input_valid:false, funding_verified:false, signer_verified:false,
            recipients_supported:false, fully_backed:false, read_only:true,
            passed:false, error_category:"input_invalid" }
    }
}

fn endpoint(raw: &str) -> Option<reqwest::Url> {
    if raw.len() > 4096 { return None; }
    let url = reqwest::Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
        && url.username().is_empty() && url.password().is_none()
        && url.query().is_none() && url.fragment().is_none()).then_some(url)
}
fn auth_valid(user: &Option<String>, password: &Option<String>) -> bool {
    match (user, password) {
        (None, None) => true,
        (Some(u), Some(p)) => !u.is_empty() && u.len() <= 8192 && p.len() <= 8192
            && !u.contains('\0') && !p.contains('\0'),
        _ => false,
    }
}
fn parse_input(raw: &str) -> Option<Input> {
    if raw.is_empty() || raw.len() > MAX_INPUT_BYTES as usize { return None; }
    let input: Input = serde_json::from_str(raw).ok()?;
    let wallet = endpoint(&input.wallet.rpc_url)?;
    endpoint(&input.node.rpc_url)?;
    let loopback = wallet.host_str()?.trim_start_matches('[').trim_end_matches(']')
        .parse::<IpAddr>().ok()?.is_loopback();
    if input.network != "testnet" || !loopback
        || !(1_000_000_001..=2_100_000_000_000_000).contains(&input.required_spendable_zatoshis)
        || input.wallet.source.is_empty() || input.wallet.source.len() > 2048
        || !input.wallet.source.bytes().all(|b|b.is_ascii_graphic())
        || input.recipients.len() > 100
        || input.recipients.iter().any(|s|s.is_empty() || s.len() > 512)
        || !auth_valid(&input.wallet.rpc_user, &input.wallet.rpc_password)
        || !auth_valid(&input.node.rpc_user, &input.node.rpc_password)
        || input.node.rpc_url_sha256.len() != 64
        || !input.node.rpc_url_sha256.bytes().all(|b|b.is_ascii_hexdigit())
        || !input.node.rpc_url_sha256.eq_ignore_ascii_case(
            &format!("{:x}", Sha256::digest(input.node.rpc_url.as_bytes())))
    { return None; }
    Some(input)
}
fn client(url: &str, user: &Option<String>, password: &Option<String>)
    -> Result<ZcashRpcClient, ()>
{
    let transport = reqwest::Client::builder().no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(Duration::from_secs(5)).timeout(Duration::from_secs(15))
        .build().map_err(|_|())?;
    let auth = user.as_ref().zip(password.as_ref()).map(|(u,p)|(u.clone(),p.clone()));
    Ok(ZcashRpcClient::with_transport(url,auth,transport))
}
fn error_category(error: ZecdFundingError) -> &'static str {
    match error {
        ZecdFundingError::IdentitySignerNotProven => "signer_unverified",
        ZecdFundingError::IdentityMismatch | ZecdFundingError::UnsupportedSource => "identity_mismatch",
        ZecdFundingError::ChainMismatch => "chain_unverified",
        ZecdFundingError::ConcurrentChange => "concurrent_change",
        ZecdFundingError::Timeout => "deadline_exceeded",
        ZecdFundingError::Unavailable | ZecdFundingError::UnsupportedRpc => "rpc_unavailable",
        ZecdFundingError::InvalidEvidence => "invalid_evidence",
        ZecdFundingError::NotReady => "wallet_not_ready",
        ZecdFundingError::ResponseTooLarge => "response_too_large",
        ZecdFundingError::EnumerationTooLarge => "enumeration_too_large",
        _ => "funding_unverified",
    }
}
async fn probe(input: &Input, report: &mut Report) {
    report.input_valid = true;
    if input.recipients.iter().any(|address|
        node_rpc::zecd_conventional::validate_testnet_recipient(address).is_err())
    {
        report.error_category = "recipient_unsupported";
        return;
    }
    report.recipients_supported = true;
    let Ok(wallet) = client(&input.wallet.rpc_url,&input.wallet.rpc_user,&input.wallet.rpc_password) else {
        report.error_category="transport_unavailable"; return;
    };
    let Ok(node) = client(&input.node.rpc_url,&input.node.rpc_user,&input.node.rpc_password) else {
        report.error_category="transport_unavailable"; return;
    };
    match collect_testnet_funding(&wallet,&input.wallet.source,&node).await {
        Ok(proof) => {
            report.funding_verified = true;
            report.signer_verified = true;
            report.fully_backed = proof.confirmed_eligible_zatoshis >= input.required_spendable_zatoshis;
            report.passed = report.fully_backed;
            report.error_category = if report.passed { "none" } else { "insufficient_backing" };
        }
        Err(error) => report.error_category = error_category(error),
    }
}

// fstat's FIFO type alone is insufficient: a named FIFO has that type too.
// Check the kernel descriptor identity without resolving or printing paths.
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
async fn main() -> ExitCode {
    // No panic payload may disclose input, even on an unexpected library fault.
    std::panic::set_hook(Box::new(|_|{}));
    let started = Instant::now();
    let mut report = Report::new();
    if std::env::args_os().skip(1).next().is_none() {
        if let Some(input) = read_pipe_config(libc::STDIN_FILENO,INPUT_LIMIT)
            .as_deref().and_then(parse_input)
        {
            if let Some(remaining) = TOTAL_LIMIT.checked_sub(started.elapsed()) {
                if tokio::time::timeout(remaining,probe(&input,&mut report)).await.is_err() {
                    report.passed = false;
                    report.error_category = "deadline_exceeded";
                }
            } else { report.error_category = "deadline_exceeded"; }
        }
    }
    // Fixed booleans and one allowlisted static category only.
    let output = serde_json::to_string(&report).unwrap_or_else(|_|String::from(
        "{\"input_valid\":false,\"funding_verified\":false,\"signer_verified\":false,\"recipients_supported\":false,\"fully_backed\":false,\"read_only\":true,\"passed\":false,\"error_category\":\"serialization_failed\"}"));
    println!("{output}");
    if report.passed { ExitCode::SUCCESS } else { ExitCode::from(1) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json,Value};
    fn private_fixture_url() -> String {
        // Construct the synthetic RFC1918 fixture so privacy scans cannot
        // mistake a checked-in address literal for production inventory.
        format!("http://{}:18232/",std::net::Ipv4Addr::new(192,168,1,2))
    }
    fn input() -> Value {
        let node = private_fixture_url();
        json!({"network":"testnet",
            "wallet":{"rpc_url":"http://127.0.0.1:28232/","rpc_user":"synthetic-user",
                "rpc_password":"synthetic-password","source":"synthetic-source"},
            "node":{"rpc_url":node,"rpc_url_sha256":format!("{:x}",Sha256::digest(node.as_bytes()))},
            "required_spendable_zatoshis":1_000_000_001_i64,"recipients":[]})
    }
    #[test]
    fn input_requires_exact_network_loopback_pin_and_full_finite_backing() {
        assert!(parse_input(&input().to_string()).is_some());
        for kind in 0..11 {
            let mut v=input();
            match kind {
                0=>v["network"]=json!("mainnet"),
                1=>v["wallet"]["rpc_url"]=json!("http://example.test/"),
                2=>v["wallet"]["rpc_url"]=json!(private_fixture_url()),
                3=>v["wallet"]["rpc_url"]=json!("http://user:pass@127.0.0.1/"),
                4=>v["wallet"]["rpc_url"]=json!("http://127.0.0.1/?secret=x"),
                5=>v["node"]["rpc_url_sha256"]=json!("0".repeat(64)),
                6=>v["node"]["rpc_password"]=json!("unpaired"),
                7=>v["required_spendable_zatoshis"]=json!(1_000_000_000),
                8=>v["required_spendable_zatoshis"]=json!(1.5),
                9=>v["wallet"]["arbitrary_method"]=json!("z_sendmany"),
                _=>v["recipients"]=json!(vec!["fixture";101]),
            }
            assert!(parse_input(&v.to_string()).is_none());
        }
        let mut v=input();
        v["wallet"]["rpc_url"]=json!("http://[::1]:28232/");
        assert!(parse_input(&v.to_string()).is_some());
    }
    #[test]
    fn fixed_output_cannot_expose_input_or_financial_values() {
        let report=Report::new();
        let v=serde_json::to_value(&report).unwrap();
        assert_eq!(v.as_object().unwrap().len(),8);
        for name in ["input_valid","funding_verified","signer_verified","recipients_supported",
            "fully_backed","read_only","passed"] { assert!(v[name].is_boolean()); }
        let text=v.to_string();
        for forbidden in ["synthetic-user","synthetic-password","synthetic-source",
            "127.0.0.1","1000000001","rpc_url"] {
            assert!(!text.contains(forbidden));
        }
        assert!(!text.contains(&private_fixture_url()));
        assert_eq!(error_category(ZecdFundingError::IdentitySignerNotProven),"signer_unverified");
        assert_eq!(error_category(ZecdFundingError::ChainMismatch),"chain_unverified");
        assert_eq!(error_category(ZecdFundingError::InvalidEvidence),"invalid_evidence");
        assert_eq!(error_category(ZecdFundingError::NotReady),"wallet_not_ready");
        assert_eq!(error_category(ZecdFundingError::ResponseTooLarge),"response_too_large");
        assert_eq!(error_category(ZecdFundingError::EnumerationTooLarge),"enumeration_too_large");
    }
    #[test]
    fn only_bounded_anonymous_pipes_are_accepted() {
        use std::io::Write;
        let mut fds=[0;2];
        assert_eq!(unsafe{libc::pipe(fds.as_mut_ptr())},0);
        let read=unsafe{File::from_raw_fd(fds[0])};
        let mut write=unsafe{File::from_raw_fd(fds[1])};
        write.write_all(b"{\"test\":true}").unwrap();
        drop(write);
        assert_eq!(read_pipe_config(read.as_raw_fd(),Duration::from_millis(50)).as_deref(),Some("{\"test\":true}"));
        let null=File::open("/dev/null").unwrap();
        assert!(read_pipe_config(null.as_raw_fd(),Duration::from_millis(50)).is_none());
    }
}
