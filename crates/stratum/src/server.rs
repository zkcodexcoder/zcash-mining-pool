use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use futures::SinkExt;
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio_util::codec::Framed;
use tracing::{debug, error, info, warn};

use crate::codec::StratumCodec;
use crate::messages::*;
use crate::session::*;

/// Events sent from stratum sessions up to the pool-core layer.
#[derive(Debug, Clone)]
pub enum StratumEvent {
    /// A miner submitted a share for validation.
    ShareSubmitted {
        session_id: String,
        request_id: serde_json::Value,
        worker_name: String,
        job_id: String,
        time: String,
        nonce_1: String,
        nonce_2: String,
        equihash_solution: String,
    },
    /// A new worker connected and authorized.
    WorkerConnected {
        session_id: String,
        worker_name: String,
        password: String,
        addr: SocketAddr,
        local_port: u16,
    },
    /// A session disconnected.
    SessionDisconnected {
        session_id: String,
    },
    /// Miner suggested a target.
    TargetSuggested {
        session_id: String,
        target: String,
    },
}

/// Response from pool-core back to a specific session about a share.
#[derive(Debug, Clone)]
pub struct ShareResponse {
    pub session_id: String,
    pub request_id: serde_json::Value,
    pub accepted: bool,
    pub error: Option<StratumError>,
}

/// Holds the state for all connected miners + broadcast channel for jobs.
/// Maximum connections allowed per IP address.
const MAX_CONNECTIONS_PER_IP: u32 = 5;

pub struct StratumServer {
    nonce_allocator: Arc<NonceAllocator>,
    /// Sender for pool events (share submissions, connections, etc.)
    event_tx: mpsc::Sender<StratumEvent>,
    /// Broadcast channel for server->miner notifications (notify, set_target).
    notify_tx: broadcast::Sender<ServerMessage>,
    /// Per-session channels for targeted responses (share accept/reject).
    session_senders: Arc<RwLock<HashMap<String, mpsc::Sender<ServerMessage>>>>,
    /// Most recent notify message, sent to new miners on subscribe.
    latest_notify: Arc<RwLock<Option<ServerMessage>>>,
    /// Connection count per IP for rate limiting.
    ip_connections: Arc<std::sync::Mutex<HashMap<IpAddr, u32>>>,
    /// Initial difficulty announced at subscribe time, keyed by listening
    /// port, as (difficulty, target hex) pairs precomputed by the caller.
    /// Pool checkers (e.g. MiningRigRentals) subscribe without authorizing,
    /// so this pre-auth announcement is the only difficulty they ever see.
    port_initial: HashMap<u16, (f64, String)>,
    /// Announced when the connection's local port has no map entry.
    fallback_initial: (f64, String),
}

impl StratumServer {
    pub fn new(
        nonce1_size: usize,
        event_tx: mpsc::Sender<StratumEvent>,
    ) -> (Self, broadcast::Receiver<ServerMessage>) {
        Self::new_with_latest_notify(nonce1_size, event_tx, Arc::new(RwLock::new(None)))
    }

    pub fn new_with_latest_notify(
        nonce1_size: usize,
        event_tx: mpsc::Sender<StratumEvent>,
        latest_notify: Arc<RwLock<Option<ServerMessage>>>,
    ) -> (Self, broadcast::Receiver<ServerMessage>) {
        let (notify_tx, notify_rx) = broadcast::channel(256);
        let server = Self {
            nonce_allocator: Arc::new(NonceAllocator::new(nonce1_size)),
            event_tx,
            notify_tx,
            session_senders: Arc::new(RwLock::new(HashMap::new())),
            latest_notify,
            ip_connections: Arc::new(std::sync::Mutex::new(HashMap::new())),
            port_initial: HashMap::new(),
            fallback_initial: (
                8192.0,
                "000042e340f98608c00000000000000000000000000000000000000000000000"
                    .to_string(),
            ),
        };
        (server, notify_rx)
    }

    /// Configure the subscribe-time difficulty announcement. `per_port` maps
    /// each listening port to a (difficulty, target hex) pair; `fallback`
    /// covers unmapped ports. The caller precomputes targets with the same
    /// difficulty→target conversion applied after authorize, so the pre-auth
    /// announcement matches the session's real starting difficulty.
    pub fn set_initial_difficulty(
        &mut self,
        per_port: HashMap<u16, (f64, String)>,
        fallback: (f64, String),
    ) {
        self.port_initial = per_port;
        self.fallback_initial = fallback;
    }

    /// Broadcast a job notification to all connected miners.
    pub fn broadcast_notify(&self, msg: ServerMessage) {
        let _ = self.notify_tx.send(msg);
    }

    /// Send a targeted message to a specific session (e.g., share response).
    ///
    /// Uses `try_send` rather than `send().await` to avoid blocking the caller
    /// on a slow miner. The validator runs in a single task and feeds all
    /// sessions; one stalled miner whose per-session channel fills must NEVER
    /// be able to wedge it. Dropping a message is preferable: the miner will
    /// just retry the next stratum exchange.
    pub async fn send_to_session(&self, session_id: &str, msg: ServerMessage) {
        let senders = self.session_senders.read().await;
        if let Some(tx) = senders.get(session_id) {
            if let Err(mpsc::error::TrySendError::Full(dropped)) = tx.try_send(msg) {
                // Audit #16: say WHAT was dropped. A dropped SubmitResult means
                // the miner never saw its accept-ACK and counts the share as a
                // reject — its local stats lie about our pool.
                let kind = match &dropped {
                    ServerMessage::SubmitResult { .. } => "submit-ack",
                    ServerMessage::Notify { .. } => "notify",
                    ServerMessage::SetTarget { .. } => "set-target",
                    ServerMessage::SetDifficulty { .. } => "set-difficulty",
                    _ => "other",
                };
                warn!(%session_id, kind, "Per-session channel full; dropping message");
            }
        }
    }

    /// Disconnect a session by closing its channel (causes the session loop to exit).
    pub async fn disconnect_session(&self, session_id: &str) {
        let mut senders = self.session_senders.write().await;
        senders.remove(session_id);
    }

    /// Start listening for miner connections.
    pub async fn listen(self: Arc<Self>, addr: &str) -> std::io::Result<()> {
        let listener = TcpListener::bind(addr).await?;
        info!(address = addr, "Stratum server listening");

        loop {
            match listener.accept().await {
                Ok((stream, peer_addr)) => {
                    // Per-IP connection limiting
                    let ip = peer_addr.ip();
                    let allowed = {
                        let mut conns = self.ip_connections.lock().unwrap();
                        let count = conns.entry(ip).or_insert(0);
                        if *count >= MAX_CONNECTIONS_PER_IP {
                            false
                        } else {
                            *count += 1;
                            true
                        }
                    };
                    if !allowed {
                        drop(stream);
                        continue;
                    }
                    info!(%peer_addr, "New miner connection");
                    let server = Arc::clone(&self);
                    let ip_conns = Arc::clone(&self.ip_connections);
                    tokio::spawn(async move {
                        if let Err(e) = server.handle_connection(stream, peer_addr).await {
                            warn!(%peer_addr, error = %e, "Session error");
                        }
                        let mut conns = ip_conns.lock().unwrap();
                        if let Some(count) = conns.get_mut(&ip) {
                            *count = count.saturating_sub(1);
                        }
                    });
                }
                Err(e) => {
                    error!(error = %e, "Accept error");
                }
            }
        }
    }

    async fn handle_connection(
        self: Arc<Self>,
        stream: TcpStream,
        peer_addr: SocketAddr,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Enable TCP keepalive to detect dead connections through NAT/proxies.
        let sock_ref = socket2::SockRef::from(&stream);
        let keepalive = socket2::TcpKeepalive::new()
            .with_time(Duration::from_secs(30))
            .with_interval(Duration::from_secs(10));
        let _ = sock_ref.set_tcp_keepalive(&keepalive);

        let local_port = stream.local_addr().map(|a| a.port()).unwrap_or(0);
        let mut framed = Framed::new(stream, StratumCodec);
        let session_id = generate_session_id();
        let nonce_1 = self.nonce_allocator.allocate();
        let mut session = MinerSession::new(session_id.clone(), nonce_1.clone());

        let (session_tx, mut session_rx) = mpsc::channel::<ServerMessage>(64);
        {
            let mut senders = self.session_senders.write().await;
            senders.insert(session_id.clone(), session_tx);
        }

        let mut notify_rx = self.notify_tx.subscribe();

        // Ping timer: send a lightweight JSON-RPC ping every 30s to keep
        // the connection alive through WebSocket proxies, NAT, and firewalls.
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
        ping_interval.tick().await; // consume the immediate first tick

        // Idle timeout: disconnect if no data received for 5 minutes.
        let idle_timeout = Duration::from_secs(300);
        let mut last_activity = tokio::time::Instant::now();

        loop {
            tokio::select! {
                // Messages from the miner
                frame = futures::StreamExt::next(&mut framed) => {
                    last_activity = tokio::time::Instant::now();
                    match frame {
                        Some(Ok(raw)) => {
                            debug!(%peer_addr, raw = ?raw, "Received from miner");
                            if let Some(request) = ClientRequest::parse(&raw) {
                                let responses = self.handle_request(
                                    &mut session,
                                    request,
                                    peer_addr,
                                    local_port,
                                ).await;
                                for msg in &responses {
                                    debug!(%peer_addr, json = %msg.to_json(), "Sending to miner");
                                }
                                for msg in responses {
                                    framed.send(msg.to_json()).await?;
                                }
                            }
                        }
                        Some(Err(e)) => {
                            warn!(%peer_addr, error = %e, "Codec error");
                            break;
                        }
                        None => {
                            info!(%peer_addr, session_id = %session.session_id, "Miner disconnected");
                            break;
                        }
                    }
                }

                // Broadcast notifications (new jobs, target changes)
                msg = notify_rx.recv() => {
                    if let Ok(msg) = msg {
                        debug!(%peer_addr, json = %msg.to_json(), "Broadcasting to miner");
                        if framed.send(msg.to_json()).await.is_err() {
                            break;
                        }
                    }
                }

                // Targeted messages for this session (share responses, set_target)
                // Returns None when channel is closed (disconnect_session was called)
                msg = session_rx.recv() => {
                    match msg {
                        Some(msg) => {
                            debug!(%peer_addr, json = %msg.to_json(), "Session msg to miner");
                            if framed.send(msg.to_json()).await.is_err() {
                                break;
                            }
                        }
                        None => {
                            info!(%peer_addr, session_id = %session.session_id, "Session disconnected by pool");
                            break;
                        }
                    }
                }

                // Periodic ping to keep connection alive through proxies
                _ = ping_interval.tick() => {
                    // Check idle timeout first
                    if last_activity.elapsed() > idle_timeout {
                        info!(%peer_addr, session_id = %session.session_id, "Idle timeout, disconnecting");
                        break;
                    }
                    // Use id:null so miners treat this as a notification, not a
                    // request/response. nheqminer was interpreting numbered IDs as
                    // rejected share responses.
                    let ping = serde_json::json!({
                        "id": null,
                        "method": "mining.ping",
                        "params": []
                    });
                    if framed.send(serde_json::to_string(&ping).unwrap()).await.is_err() {
                        break;
                    }
                }
            }
        }

        // Cleanup
        {
            let mut senders = self.session_senders.write().await;
            senders.remove(&session_id);
        }
        let _ = self.event_tx.send(StratumEvent::SessionDisconnected {
            session_id,
        }).await;

        Ok(())
    }

    async fn handle_request(
        &self,
        session: &mut MinerSession,
        request: ClientRequest,
        peer_addr: SocketAddr,
        local_port: u16,
    ) -> Vec<ServerMessage> {
        let mut responses = Vec::new();

        match request {
            ClientRequest::Subscribe { id, user_agent, .. } => {
                if session.subscribed {
                    // Duplicate subscribe — reply with same session info but don't reset state.
                    debug!(%peer_addr, %user_agent, "Duplicate subscribe, replying with existing session");
                    responses.push(ServerMessage::SubscribeResult {
                        id,
                        session_id: session.session_id.clone(),
                        nonce_1: session.nonce_1.clone(),
                        nonce2_size: self.nonce_allocator.nonce2_size(),
                    });
                } else {
                    info!(%peer_addr, %user_agent, "Miner subscribing");
                    session.subscribed = true;
                    responses.push(ServerMessage::SubscribeResult {
                        id,
                        session_id: session.session_id.clone(),
                        nonce_1: session.nonce_1.clone(),
                        nonce2_size: self.nonce_allocator.nonce2_size(),
                    });
                    // Announce this port's configured starting difficulty, in
                    // both formats so all miners understand it. Pool-core sends
                    // the authoritative target after authorize (password d=
                    // overrides apply there); pool checkers that never
                    // authorize (e.g. MiningRigRentals) only ever see this one.
                    let (difficulty, target) = self
                        .port_initial
                        .get(&local_port)
                        .unwrap_or(&self.fallback_initial)
                        .clone();
                    responses.push(ServerMessage::SetDifficulty { difficulty });
                    responses.push(ServerMessage::SetTarget { target });
                    let latest = self.latest_notify.read().await;
                    if let Some(ref notify) = *latest {
                        // A new subscriber has no prior work, so its first job
                        // must be clean. The cached broadcast notify is usually
                        // clean_jobs=false (the race-to-tip full-template path
                        // sets it that way so existing miners keep in-flight
                        // shares) — override it for this new connection so the
                        // miner starts hashing immediately instead of waiting
                        // for the next new-block notify.
                        let mut first = notify.clone();
                        if let ServerMessage::Notify { clean_jobs, .. } = &mut first {
                            *clean_jobs = true;
                        }
                        responses.push(first);
                    }
                }
            }

            ClientRequest::Authorize { id, worker_name, worker_password } => {
                if !session.subscribed {
                    responses.push(ServerMessage::AuthorizeResult {
                        id,
                        authorized: false,
                        error: Some(StratumError::not_subscribed()),
                    });
                    return responses;
                }

                // SECURITY: validate worker_name to a safe charset before it is
                // stored (as miners.address / workers.name) and later rendered in
                // the admin/dashboard UI. Legit Zcash addresses and worker labels
                // are alphanumeric with . _ - ; rejecting anything else prevents a
                // stored-XSS payload (< > " ' & ...) reaching an operator's admin
                // session, and blocks empty-name / junk-row / unbounded-INSERT abuse.
                let wn_ok = !worker_name.is_empty()
                    && worker_name.len() <= 256
                    && worker_name
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
                if !wn_ok {
                    responses.push(ServerMessage::AuthorizeResult {
                        id,
                        authorized: false,
                        error: Some(StratumError::other(
                            "Invalid worker name (allowed characters: A-Z a-z 0-9 . _ -)",
                        )),
                    });
                    return responses;
                }

                session.authorize_worker(&worker_name);
                info!(%peer_addr, %worker_name, "Worker authorized");

                let _ = self.event_tx.send(StratumEvent::WorkerConnected {
                    session_id: session.session_id.clone(),
                    worker_name: worker_name.clone(),
                    password: worker_password,
                    addr: peer_addr,
                    local_port,
                }).await;

                responses.push(ServerMessage::AuthorizeResult {
                    id,
                    authorized: true,
                    error: None,
                });
            }

            ClientRequest::Submit {
                id, worker_name, job_id, time, nonce_2, equihash_solution,
            } => {
                if !session.is_worker_authorized(&worker_name) {
                    responses.push(ServerMessage::SubmitResult {
                        id,
                        accepted: false,
                        error: Some(StratumError::unauthorized()),
                    });
                    return responses;
                }

                let _ = self.event_tx.send(StratumEvent::ShareSubmitted {
                    session_id: session.session_id.clone(),
                    request_id: id,
                    worker_name,
                    job_id,
                    time,
                    nonce_1: session.nonce_1.clone(),
                    nonce_2,
                    equihash_solution,
                }).await;

                // Response will be sent asynchronously via session channel
                // after pool-core validates the share.
            }

            ClientRequest::SuggestTarget { id: _, target } => {
                let _ = self.event_tx.send(StratumEvent::TargetSuggested {
                    session_id: session.session_id.clone(),
                    target,
                }).await;
                // Server responds via mining.set_target asynchronously
            }

            ClientRequest::ExtranonceSubscribe { id } => {
                debug!(%peer_addr, "Extranonce subscribe (acknowledged)");
                responses.push(ServerMessage::AuthorizeResult {
                    id,
                    authorized: true,
                    error: None,
                });
            }

            ClientRequest::Unknown { id, method } => {
                warn!(%peer_addr, %method, "Unknown stratum method");
                responses.push(ServerMessage::SubmitResult {
                    id,
                    accepted: false,
                    error: Some(StratumError::other(&format!("Unknown method: {method}"))),
                });
            }
        }

        responses
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::MinerSession;

    fn server_with_ports() -> StratumServer {
        let (event_tx, _event_rx) = mpsc::channel(8);
        let (mut server, _notify_rx) = StratumServer::new(4, event_tx);
        let mut per_port = HashMap::new();
        per_port.insert(3336u16, (10_000_000.0, "aa".repeat(32)));
        server.set_initial_difficulty(per_port, (100.0, "bb".repeat(32)));
        server
    }

    async fn subscribe_on(server: &StratumServer, local_port: u16) -> Vec<ServerMessage> {
        let mut session = MinerSession::new("s1".into(), "de810000".into());
        server
            .handle_request(
                &mut session,
                ClientRequest::Subscribe {
                    id: serde_json::json!(1),
                    user_agent: "test/1.0".into(),
                    session_id: None,
                    host: None,
                    port: None,
                },
                "127.0.0.1:55555".parse().unwrap(),
                local_port,
            )
            .await
    }

    fn announced(responses: &[ServerMessage]) -> (f64, String) {
        let mut diff = None;
        let mut target = None;
        for r in responses {
            match r {
                ServerMessage::SetDifficulty { difficulty } => diff = Some(*difficulty),
                ServerMessage::SetTarget { target: t } => target = Some(t.clone()),
                _ => {}
            }
        }
        (diff.expect("no set_difficulty sent"), target.expect("no set_target sent"))
    }

    #[tokio::test]
    async fn subscribe_announces_port_initial_difficulty() {
        let server = server_with_ports();
        let (diff, target) = announced(&subscribe_on(&server, 3336).await);
        assert_eq!(diff, 10_000_000.0);
        assert_eq!(target, "aa".repeat(32));
    }

    #[tokio::test]
    async fn subscribe_falls_back_on_unmapped_port() {
        let server = server_with_ports();
        let (diff, target) = announced(&subscribe_on(&server, 1234).await);
        assert_eq!(diff, 100.0);
        assert_eq!(target, "bb".repeat(32));
    }
}
