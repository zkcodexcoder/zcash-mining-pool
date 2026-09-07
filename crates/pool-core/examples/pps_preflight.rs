//! Read-only native PPS admission probe, suitable for an owner-run wrapper.
//!
//! Usage: pps_preflight --config /absolute/path/to/pool.toml [--require-policy]
//! Or: an owner-controlled wrapper pipes config to --config-stdin [--require-policy].
//! Accepts only an owner-only existing file or a bounded anonymous stdin pipe.
//! There are no URL, auth,
//! wallet, method, reference-server or network overrides on the command line.
//! A missing PPS policy permits a zero-fee MATHEMATICAL quote probe only; it
//! does not authorize PPS. Startup wrappers must pass --require-policy.
//! Emits only one fixed-schema JSON object containing booleans. Never emits
//! configuration, RPC errors, addresses, hashes, subsidy or financial amounts.

use node_rpc::ZcashRpcClient;
use pool_core::pps_chain::verify_pps_chain;
use pool_core::pps_economics::{validate_template_target, validated_miner_subsidy};
use pool_db::pps_policy::PpsPolicy;
use rewards::{quote_standard_pps, PpsNetwork, PpsQuoteInput};
use serde::{Deserialize, Serialize};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, FromRawFd, RawFd};
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

const MAX_CONFIG_BYTES: u64 = 1_048_576;
const CONFIG_PIPE_LIMIT: Duration = Duration::from_secs(5);
const LOCAL_CALL_LIMIT: Duration = Duration::from_secs(5);
const PROBE_LIMIT: Duration = Duration::from_secs(55);

// Deliberately not Debug/Serialize: credential-bearing inputs cannot be used
// accidentally as structured logging or as the public probe result.
#[derive(Deserialize)]
struct Config {
    pool: Pool,
    node: Node,
    difficulty: Difficulty,
    pplns: Option<RewardMode>,
    pps: Option<PpsPolicy>,
}
#[derive(Deserialize)]
struct Pool {
    #[serde(default = "default_network")]
    network: String,
}
fn default_network() -> String { "testnet".into() }
#[derive(Deserialize)]
struct Node {
    rpc_url: String,
    rpc_user: Option<String>,
    rpc_password: Option<String>,
}
#[derive(Deserialize)]
struct Difficulty {
    initial_target: String,
}
#[derive(Deserialize)]
struct RewardMode {
    mode: Option<String>,
}
#[derive(Deserialize)]
struct Tip {
    chain: String,
    blocks: u64,
    bestblockhash: String,
}

#[derive(Default, Serialize)]
struct Report {
    read_only_probe_passed: bool,
    owner_only_config: bool,
    anonymous_pipe_config: bool,
    config_valid: bool,
    policy_required: bool,
    policy_present: bool,
    policy_valid: bool,
    pps_mode_configured: bool,
    fixed_target_valid: bool,
    independent_chain_agreement: bool,
    template_received: bool,
    network_target_consistent: bool,
    miner_subsidy_valid: bool,
    exact_quote_valid: bool,
    template_still_current: bool,
    lease_still_current: bool,
    deadline_exceeded: bool,
}

struct Args {
    config: ConfigSource,
    require_policy: bool,
}
enum ConfigSource {
    OwnerOnlyFile(PathBuf),
    AnonymousStdin,
}
fn args(values: impl Iterator<Item = std::ffi::OsString>) -> Option<Args> {
    let mut values = values;
    let config = match values.next()?.to_str()? {
        "--config" => {
            let path = PathBuf::from(values.next()?);
            if !path.is_absolute() {
                return None;
            }
            ConfigSource::OwnerOnlyFile(path)
        }
        "--config-stdin" => ConfigSource::AnonymousStdin,
        _ => return None,
    };
    let require_policy = match values.next() {
        None => false,
        Some(flag) if flag.as_os_str() == "--require-policy" => true,
        _ => return None,
    };
    if values.next().is_some() {
        return None;
    }
    Some(Args {
        config,
        require_policy,
    })
}

fn read_config(path: &Path) -> Option<String> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(path)
        .ok()?;
    let metadata = file.metadata().ok()?;
    // SAFETY: geteuid has no arguments, pointer access or side effects.
    let owner = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != owner
        || metadata.mode() & 0o077 != 0
        || metadata.len() == 0
        || metadata.len() > MAX_CONFIG_BYTES
    {
        return None;
    }
    let mut text = String::new();
    file.by_ref()
        .take(MAX_CONFIG_BYTES + 1)
        .read_to_string(&mut text)
        .ok()?;
    let after = file.metadata().ok()?;
    if text.len() as u64 != metadata.len()
        || after.len() != metadata.len()
        || after.mode() != metadata.mode()
        || after.uid() != metadata.uid()
        || after.mtime() != metadata.mtime()
        || after.mtime_nsec() != metadata.mtime_nsec()
    {
        return None;
    }
    Some(text)
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
        if bytes.len() == MAX_CONFIG_BYTES as usize {
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
        let capacity = buffer.len().min(MAX_CONFIG_BYTES as usize - bytes.len());
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

fn valid_node(node: &Node) -> bool {
    valid_node_at_target(node, None)
}

fn valid_node_at_target(node: &Node, expected_rpc_sha256: Option<&str>) -> bool {
    use sha2::{Digest, Sha256};
    let Ok(url) = reqwest::Url::parse(&node.rpc_url) else {
        return false;
    };
    let private = url.host_str().is_some_and(|host| {
        host == "localhost"
            || host
                .trim_start_matches('[')
                .trim_end_matches(']')
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| {
                    ip.is_loopback()
                        || matches!(ip,
                    std::net::IpAddr::V4(v4) if v4.is_private())
                })
    });
    // The fixed owner-controlled wrapper may bind the probe to the exact
    // existing configured endpoint without placing that endpoint in argv/logs.
    // A public/DNS target is never accepted without this exact pin. This is
    // a read-only probe admission, not permission to change any listener.
    let pinned = expected_rpc_sha256.is_some_and(|pin| {
        pin.len() == 64 && pin.bytes().all(|b| b.is_ascii_hexdigit())
            && pin.eq_ignore_ascii_case(&hex::encode(Sha256::digest(node.rpc_url.as_bytes())))
    });
    (private || pinned)
        && url.host_str().is_some()
        && matches!(url.scheme(), "http" | "https")
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && matches!(
            (&node.rpc_user, &node.rpc_password),
            (None, None) | (Some(_), Some(_))
        )
}
fn current_template(tip: &Tip, network: PpsNetwork, height: u64, parent: &str) -> bool {
    let network_matches = match network {
        PpsNetwork::Mainnet => matches!(tip.chain.as_str(), "main" | "mainnet"),
        PpsNetwork::Testnet => matches!(tip.chain.as_str(), "test" | "testnet"),
    };
    network_matches
        && tip.blocks.checked_add(1) == Some(height)
        && parent.len() == 64
        && parent.bytes().all(|c| c.is_ascii_hexdigit())
        && tip.bestblockhash.len() == 64
        && tip.bestblockhash.bytes().all(|c| c.is_ascii_hexdigit())
        && parent.eq_ignore_ascii_case(&tip.bestblockhash)
}

fn probe_rpc(node: &Node) -> Option<ZcashRpcClient> {
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .connect_timeout(LOCAL_CALL_LIMIT)
        .timeout(LOCAL_CALL_LIMIT)
        .build()
        .ok()?;
    let auth = match (&node.rpc_user, &node.rpc_password) {
        (Some(user), Some(password)) => Some((user.clone(), password.clone())),
        (None, None) => None,
        _ => return None,
    };
    Some(ZcashRpcClient::with_transport(&node.rpc_url, auth, http))
}

async fn probe(config: Config, report: &mut Report, expected_rpc_sha256: Option<String>) {
    let Ok(network) = config.pool.network.parse::<PpsNetwork>() else {
        return;
    };
    if !valid_node_at_target(&config.node, expected_rpc_sha256.as_deref()) {
        return;
    }
    report.policy_present = config.pps.is_some();
    report.policy_valid = config
        .pps
        .as_ref()
        .is_some_and(|p| p.validate(&config.pool.network).is_ok());
    report.pps_mode_configured = config
        .pplns
        .as_ref()
        .and_then(|p| p.mode.as_ref())
        .is_some_and(|mode| mode.eq_ignore_ascii_case("pps"));
    if report.policy_present && !report.policy_valid {
        return;
    }
    let fee_bps = config.pps.as_ref().map_or(0, |p| p.fee_bps);
    let Ok(assigned) = pool_core::parse_target(&config.difficulty.initial_target) else {
        return;
    };
    report.fixed_target_valid = stratum::FixedShareTarget::new(assigned).is_ok();
    if !report.fixed_target_valid {
        return;
    }
    report.config_valid = true;
    let Some(rpc) = probe_rpc(&config.node) else {
        return;
    };
    let (lease, template) = tokio::join!(
        verify_pps_chain(&rpc, network),
        tokio::time::timeout(LOCAL_CALL_LIMIT, rpc.get_block_template())
    );
    report.independent_chain_agreement = lease.is_ok();
    report.template_received = matches!(&template, Ok(Ok(_)));
    let (Ok(lease), Ok(Ok(template))) = (lease, template) else {
        return;
    };
    let Ok(target) = rewards::parse_target_be(&template.target) else {
        return;
    };
    report.network_target_consistent = validate_template_target(&template.bits, &target).is_ok();
    if !report.network_target_consistent {
        return;
    }
    let Ok(Ok(subsidy)) = tokio::time::timeout(
        LOCAL_CALL_LIMIT,
        validated_miner_subsidy(&rpc, network, template.height),
    )
    .await
    else {
        return;
    };
    report.miner_subsidy_valid = true;
    report.exact_quote_valid = quote_standard_pps(&PpsQuoteInput {
        network,
        height: template.height,
        network_target_be: target,
        assigned_share_target_be: assigned,
        miner_subsidy_zats: subsidy,
        fee_bps,
    })
    .is_ok();
    if !report.exact_quote_valid {
        return;
    }
    let Ok(Ok(tip)) = tokio::time::timeout(
        LOCAL_CALL_LIMIT,
        rpc.call_raw::<Tip>("getblockchaininfo", serde_json::json!([])),
    )
    .await
    else {
        return;
    };
    report.template_still_current =
        current_template(&tip, network, template.height, &template.previousblockhash);
    let now = chrono::Utc::now().timestamp();
    report.lease_still_current = lease.network == config.pool.network
        && lease.agreeing_references == 2
        && !lease.disagreement
        && now >= lease.checked_at_unix
        && now <= lease.valid_until_unix
        && lease.valid_until_unix.checked_sub(lease.checked_at_unix) == Some(90);
    report.read_only_probe_passed = report.template_still_current
        && report.lease_still_current
        && (!report.policy_required || (report.policy_valid && report.pps_mode_configured));
}

#[tokio::main]
async fn main() -> ExitCode {
    // Never let a dependency panic print inputs or RPC errors. An unexpected
    // panic still exits nonzero, so a wrapper must treat absent JSON as failure.
    std::panic::set_hook(Box::new(|_| {}));
    let mut report = Report::default();
    if let Some(arguments) = args(std::env::args_os().skip(1)) {
        report.policy_required = arguments.require_policy;
        let text = match arguments.config {
            ConfigSource::OwnerOnlyFile(path) => {
                let text = read_config(&path);
                report.owner_only_config = text.is_some();
                text
            }
            ConfigSource::AnonymousStdin => {
                let text = read_pipe_config(libc::STDIN_FILENO, CONFIG_PIPE_LIMIT);
                report.anonymous_pipe_config = text.is_some();
                text
            }
        };
        if let Some(text) = text {
            if let Ok(config) = toml::from_str::<Config>(&text) {
                drop(text);
                let expected = std::env::var("PPS_PROBE_EXPECTED_RPC_SHA256").ok();
                if tokio::time::timeout(PROBE_LIMIT, probe(config, &mut report, expected))
                    .await
                    .is_err()
                {
                    report.deadline_exceeded = true;
                    report.read_only_probe_passed = false;
                }
            }
        }
    }
    let passed = report.read_only_probe_passed;
    match serde_json::to_string(&report) {
        Ok(json) => println!("{json}"),
        Err(_) => return ExitCode::from(2),
    }
    if passed {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(2)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let path = std::env::temp_dir().join(format!(
                "pps-preflight-test-{}-{}",
                std::process::id(),
                rand::random::<u64>()
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            // Only exact test-owned files, never recursive/broad cleanup.
            for name in ["config.toml", "link.toml", "fifo"] {
                let _ = std::fs::remove_file(self.0.join(name));
            }
            let _ = std::fs::remove_dir(&self.0);
        }
    }
    #[test]
    fn configuration_requires_private_regular_file_and_rejects_symlinks_and_fifos() {
        let fixture = Fixture::new();
        let path = fixture.0.join("config.toml");
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        file.write_all(b"synthetic-data").unwrap();
        assert_eq!(read_config(&path).as_deref(), Some("synthetic-data"));
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
        assert!(read_config(&path).is_none());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o664)).unwrap();
        assert!(read_config(&path).is_none());
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        std::os::unix::fs::symlink(&path, fixture.0.join("link.toml")).unwrap();
        assert!(read_config(&fixture.0.join("link.toml")).is_none());
        assert!(read_config(&fixture.0).is_none());
        let fifo =
            std::ffi::CString::new(fixture.0.join("fifo").as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: CString is NUL-terminated and lives across the system call.
        assert_eq!(unsafe { libc::mkfifo(fifo.as_ptr(), 0o600) }, 0);
        assert!(read_config(&fixture.0.join("fifo")).is_none());
        file.set_len(MAX_CONFIG_BYTES + 1).unwrap();
        assert!(read_config(&path).is_none());
    }
    #[test]
    fn no_endpoint_or_secret_overrides_in_arguments() {
        let parse = |items: &[&str]| args(items.iter().map(std::ffi::OsString::from));
        assert!(parse(&["--config", "/synthetic/pool.toml"]).is_some());
        assert!(matches!(
            parse(&["--config-stdin"]).unwrap().config,
            ConfigSource::AnonymousStdin
        ));
        assert!(
            parse(&["--config-stdin", "--require-policy"])
                .unwrap()
                .require_policy
        );
        assert!(
            parse(&["--config", "/synthetic/pool.toml", "--require-policy"])
                .unwrap()
                .require_policy
        );
        for values in [
            vec!["--url", "http://127.0.0.1:1"],
            vec!["--config", "relative"],
            vec!["--config-stdin", "extra"],
            vec!["--config-stdin", "--config", "/synthetic/pool.toml"],
            vec!["--config", "/synthetic/pool.toml", "--config-stdin"],
            vec!["--config-stdin", "--require-policy", "--require-policy"],
            vec![
                "--config",
                "/synthetic/pool.toml",
                "--password",
                "synthetic",
            ],
            vec![
                "--config",
                "/synthetic/pool.toml",
                "--require-policy",
                "extra",
            ],
        ] {
            assert!(parse(&values).is_none());
        }
    }

    fn anonymous_pipe() -> (File, File) {
        let mut fds = [-1; 2];
        // SAFETY: fds is writable for the two returned owned descriptors.
        assert_eq!(unsafe { libc::pipe(fds.as_mut_ptr()) }, 0);
        // SAFETY: each newly created fd is owned by exactly one File.
        unsafe { (File::from_raw_fd(fds[0]), File::from_raw_fd(fds[1])) }
    }

    #[test]
    fn anonymous_stdin_accepts_small_and_exact_limit_utf8_without_altering_flags() {
        for input in [
            b"synthetic-config".to_vec(),
            vec![b'x'; MAX_CONFIG_BYTES as usize],
        ] {
            let (read, mut write) = anonymous_pipe();
            let expected = input.clone();
            let before = unsafe { libc::fcntl(read.as_raw_fd(), libc::F_GETFL) };
            let producer = std::thread::spawn(move || write.write_all(&input));
            let result = read_pipe_config(read.as_raw_fd(), CONFIG_PIPE_LIMIT);
            producer.join().unwrap().unwrap();
            assert_eq!(result.unwrap().as_bytes(), expected);
            assert_eq!(
                unsafe { libc::fcntl(read.as_raw_fd(), libc::F_GETFL) },
                before
            );
        }
    }

    #[test]
    fn anonymous_stdin_rejects_oversize_empty_and_non_utf8_and_reads_at_most_limit() {
        for input in [
            Vec::new(),
            vec![0xff],
            vec![b'x'; MAX_CONFIG_BYTES as usize + 1],
        ] {
            let (mut read, mut write) = anonymous_pipe();
            let oversized = input.len() > MAX_CONFIG_BYTES as usize;
            let producer = std::thread::spawn(move || write.write_all(&input));
            assert!(read_pipe_config(read.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
            producer.join().unwrap().unwrap();
            if oversized {
                let mut unread = Vec::new();
                read.read_to_end(&mut unread).unwrap();
                assert_eq!(
                    unread,
                    vec![b'x'],
                    "oversize detection must not consume a MAX+1 byte"
                );
            }
        }
    }

    #[test]
    fn anonymous_stdin_requires_eof_and_rejects_idle_or_continuing_producer() {
        let (read, mut write) = anonymous_pipe();
        let started = std::time::Instant::now();
        assert!(read_pipe_config(read.as_raw_fd(), Duration::from_millis(25)).is_none());
        assert!(started.elapsed() < Duration::from_secs(1));
        write.write_all(b"complete-looking-but-no-eof").unwrap();
        assert!(read_pipe_config(read.as_raw_fd(), Duration::from_millis(25)).is_none());
        assert!(read_pipe_config(write.as_raw_fd(), Duration::from_millis(25)).is_none());
    }

    #[test]
    fn anonymous_stdin_rejects_regular_files_named_fifos_sockets_and_terminals() {
        let fixture = Fixture::new();
        let file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .read(true)
            .mode(0o600)
            .open(fixture.0.join("config.toml"))
            .unwrap();
        assert!(read_pipe_config(file.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        let path = fixture.0.join("fifo");
        let fifo_name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: fifo_name is a valid NUL-terminated test-owned path.
        assert_eq!(unsafe { libc::mkfifo(fifo_name.as_ptr(), 0o600) }, 0);
        let fifo = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .unwrap();
        assert!(read_pipe_config(fifo.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        // Removing the pathname must not turn a named-FIFO vnode into an
        // acceptable anonymous pipe (important for pathname-based shortcuts).
        std::fs::remove_file(&path).unwrap();
        assert!(read_pipe_config(fifo.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(read_pipe_config(socket.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        let (mut master, mut slave) = (-1, -1);
        // SAFETY: two writable fd slots, null optional name/settings pointers.
        assert_eq!(
            unsafe {
                libc::openpty(
                    &mut master,
                    &mut slave,
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                )
            },
            0
        );
        // SAFETY: openpty returned distinct owned descriptors.
        let (master, slave) = unsafe { (File::from_raw_fd(master), File::from_raw_fd(slave)) };
        assert!(read_pipe_config(master.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        assert!(read_pipe_config(slave.as_raw_fd(), CONFIG_PIPE_LIMIT).is_none());
        assert!(read_pipe_config(-1, CONFIG_PIPE_LIMIT).is_none());
    }
    #[test]
    fn output_is_fixed_booleans_not_input_values() {
        let value = serde_json::to_value(Report::default()).unwrap();
        assert_eq!(value.as_object().unwrap().len(), 17);
        assert!(value
            .as_object()
            .unwrap()
            .values()
            .all(serde_json::Value::is_boolean));
    }
    #[test]
    fn node_endpoint_requires_private_transport_and_paired_auth() {
        let mut node = Node {
            rpc_url: "http://127.0.0.1:8232".into(),
            rpc_user: None,
            rpc_password: None,
        };
        assert!(valid_node(&node));
        node.rpc_user = Some("synthetic".into());
        assert!(!valid_node(&node));
        node.rpc_password = Some("synthetic".into());
        assert!(valid_node(&node));
        for url in [
            "http://example.invalid:8232",
            "file:///tmp/fake",
            "http://synthetic@127.0.0.1:8232",
            "http://127.0.0.1:8232/?method=other",
        ] {
            node.rpc_url = url.into();
            assert!(!valid_node(&node));
        }
        for octets in [[8, 8, 8, 8], [169, 254, 169, 254], [100, 64, 0, 1]] {
            node.rpc_url = format!("http://{}:8232", std::net::Ipv4Addr::from(octets));
            assert!(!valid_node(&node));
        }
        node.rpc_url = "http://[::1]:8232".into();
        assert!(valid_node(&node));
        for octets in [[10, 1, 2, 3], [172, 16, 0, 1], [192, 168, 1, 2]] {
            node.rpc_url = format!("http://{}:8232", std::net::Ipv4Addr::from(octets));
            assert!(valid_node(&node));
        }
    }
    #[test]
    fn final_tip_binds_network_height_and_parent() {
        let hash = "ab".repeat(32);
        let mut tip = Tip {
            chain: "test".into(),
            blocks: 1000,
            bestblockhash: hash.clone(),
        };
        assert!(current_template(&tip, PpsNetwork::Testnet, 1001, &hash));
        assert!(!current_template(&tip, PpsNetwork::Mainnet, 1001, &hash));
        assert!(!current_template(&tip, PpsNetwork::Testnet, 1002, &hash));
        tip.bestblockhash = "cd".repeat(32);
        assert!(!current_template(&tip, PpsNetwork::Testnet, 1001, &hash));
    }
    #[test]
    fn exact_pin_admits_only_the_existing_nonlocal_target() {
        use sha2::{Digest, Sha256};
        let mut node = Node { rpc_url: "https://node.example.invalid/rpc".into(), rpc_user: None, rpc_password: None };
        let pin = hex::encode(Sha256::digest(node.rpc_url.as_bytes()));
        assert!(!valid_node(&node));
        assert!(valid_node_at_target(&node, Some(&pin)));
        node.rpc_url = "https://different.example.invalid/rpc".into();
        assert!(!valid_node_at_target(&node, Some(&pin)));
        assert!(!valid_node_at_target(&node, Some("malformed")));
        node.rpc_url = "https://synthetic@node.example.invalid/rpc".into();
        let pin = hex::encode(Sha256::digest(node.rpc_url.as_bytes()));
        assert!(!valid_node_at_target(&node, Some(&pin)));
        let config: Config = toml::from_str("[pool]\n[node]\nrpc_url='http://127.0.0.1:8232'\n[difficulty]\ninitial_target='01'\n").unwrap();
        assert_eq!(config.pool.network, "testnet");
    }
    #[test]
    fn config_without_pps_supports_only_read_only_mathematics() {
        let text = "[pool]\nnetwork='testnet'\n[node]\nrpc_url='http://127.0.0.1:8232'\nrpc_user='synthetic'\nrpc_password='synthetic'\n[difficulty]\ninitial_target='01'\n[payout]\nwallet_rpc_password='ignored-synthetic'\n";
        let config: Config = toml::from_str(text).unwrap();
        assert!(config.pps.is_none());
        assert!(config.pplns.is_none());
        assert!(valid_node(&config.node));
    }
    #[tokio::test]
    async fn probe_transport_refuses_redirects() {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 4096];
            let _ = stream.read(&mut buffer).await.unwrap();
            let response = format!("HTTP/1.1 302 Found\r\nContent-Length: 0\r\nConnection: close\r\nLocation: http://{address}/redirected\r\n\r\n");
            stream.write_all(response.as_bytes()).await.unwrap();
            stream.shutdown().await.unwrap();
            tokio::time::timeout(Duration::from_millis(200), listener.accept())
                .await
                .is_err()
        });
        let node = Node {
            rpc_url: format!("http://{address}"),
            rpc_user: None,
            rpc_password: None,
        };
        let rpc = probe_rpc(&node).unwrap();
        assert!(rpc.get_block_hash(1).await.is_err());
        assert!(server.await.unwrap());
    }
}
