use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{Html, IntoResponse, Json, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use serde::{Deserialize, Serialize};
use crate::handlers::AppState;

const ZATOSHIS_PER_ZEC: f64 = 100_000_000.0;

#[derive(Serialize)]
struct SystemStats {
    load_1m: f64,
    load_5m: f64,
    load_15m: f64,
    cpu_count: usize,
    mem_total_mb: u64,
    mem_used_mb: u64,
    mem_percent: f64,
    swap_total_mb: u64,
    swap_used_mb: u64,
    disk_total_gb: f64,
    disk_used_gb: f64,
    disk_percent: f64,
    db_size_mb: f64,
    open_fds: usize,
    fd_limit: usize,
}

fn read_system_stats() -> Option<SystemStats> {
    // Only works on Linux
    if !cfg!(target_os = "linux") {
        return None;
    }

    // Load average
    let loadavg = std::fs::read_to_string("/proc/loadavg").ok()?;
    let parts: Vec<&str> = loadavg.split_whitespace().collect();
    let load_1m: f64 = parts.first()?.parse().ok()?;
    let load_5m: f64 = parts.get(1)?.parse().ok()?;
    let load_15m: f64 = parts.get(2)?.parse().ok()?;

    // CPU count
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu_count = cpuinfo.lines().filter(|l| l.starts_with("processor")).count().max(1);

    // Memory
    let meminfo = std::fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let parse_kb = |key: &str| -> u64 {
        meminfo
            .lines()
            .find(|l| l.starts_with(key))
            .and_then(|l| l.split_whitespace().nth(1))
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let mem_total_kb = parse_kb("MemTotal:");
    let mem_available_kb = parse_kb("MemAvailable:");
    let swap_total_kb = parse_kb("SwapTotal:");
    let swap_free_kb = parse_kb("SwapFree:");

    let mem_total_mb = mem_total_kb / 1024;
    let mem_used_mb = mem_total_kb.saturating_sub(mem_available_kb) / 1024;
    let mem_percent = if mem_total_kb > 0 {
        (mem_total_kb - mem_available_kb) as f64 / mem_total_kb as f64 * 100.0
    } else {
        0.0
    };
    let swap_total_mb = swap_total_kb / 1024;
    let swap_used_mb = swap_total_kb.saturating_sub(swap_free_kb) / 1024;

    // Disk usage via libc statvfs
    let disk_path = std::ffi::CString::new(".").ok()?;
    let (disk_total_gb, disk_used_gb, disk_percent) = unsafe {
        let mut stat: libc::statvfs = std::mem::zeroed();
        if libc::statvfs(disk_path.as_ptr(), &mut stat) == 0 {
            let block_size = stat.f_frsize as f64;
            let total = stat.f_blocks as f64 * block_size;
            let used = total - (stat.f_bfree as f64 * block_size);
            let gb = 1024.0 * 1024.0 * 1024.0;
            let pct = if total > 0.0 { used / total * 100.0 } else { 0.0 };
            (total / gb, used / gb, pct)
        } else {
            (0.0, 0.0, 0.0)
        }
    };

    // DB size
    let db_size_mb = std::fs::metadata("pool.db")
        .map(|m| m.len() as f64 / (1024.0 * 1024.0))
        .unwrap_or(0.0);

    // Open file descriptors
    let open_fds = std::fs::read_dir("/proc/self/fd")
        .map(|d| d.count())
        .unwrap_or(0);

    // FD limit
    let fd_limit = std::fs::read_to_string("/proc/self/limits")
        .ok()
        .and_then(|s| {
            s.lines()
                .find(|l| l.starts_with("Max open files"))
                .and_then(|l| {
                    l.split_whitespace()
                        .nth(3) // "Max open files" (3 words) then soft limit
                        .and_then(|v| v.parse::<usize>().ok())
                })
        })
        .unwrap_or(0);

    Some(SystemStats {
        load_1m,
        load_5m,
        load_15m,
        cpu_count,
        mem_total_mb,
        mem_used_mb,
        mem_percent,
        swap_total_mb,
        swap_used_mb,
        disk_total_gb,
        disk_used_gb,
        disk_percent,
        db_size_mb,
        open_fds,
        fd_limit,
    })
}
const SESSION_COOKIE_NAME: &str = "admin_session";
const SESSION_MAX_AGE_SECS: i64 = 86400; // 24 hours

/// Read-only snapshot of pool configuration (no secrets).
#[derive(Clone, Serialize)]
pub struct PoolConfigView {
    pub pool_name: String,
    pub pool_fee: f64,
    pub network: String,
    pub stratum_ports: Vec<String>,
    pub difficulty_multiplier: f64,
    pub min_payout_zec: f64,
    pub maturity_confirmations: u64,
    pub pool_address: Option<String>,
    pub mining_address: Option<String>,
    pub node_rpc_url: String,
    pub wallet_rpc_url: Option<String>,
    pub coinbase_tag: Option<String>,
    pub payout_interval_secs: u64,
}

/// Shared state for the admin server.
#[derive(Clone)]
pub struct AdminState {
    pub app: AppState,
    signing_key: [u8; 32],
    pub config_view: PoolConfigView,
    pub config_path: String,
    pub started_at: i64,
}

impl AdminState {
    pub fn new(
        app: AppState,
        password: &str,
        config_view: PoolConfigView,
        config_path: String,
    ) -> Self {
        let signing_key = derive_signing_key(password);
        let started_at = chrono::Utc::now().timestamp();
        Self {
            app,
            signing_key,
            config_view,
            config_path,
            started_at,
        }
    }
}

fn derive_signing_key(password: &str) -> [u8; 32] {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .personal(b"zcpool-admin-key")
        .hash(password.as_bytes());
    let mut key = [0u8; 32];
    key.copy_from_slice(hash.as_bytes());
    key
}

fn compute_mac(key: &[u8; 32], message: &[u8]) -> String {
    let hash = blake2b_simd::Params::new()
        .hash_length(32)
        .key(key)
        .hash(message);
    hex::encode(hash.as_bytes())
}

fn make_session_cookie(key: &[u8; 32]) -> String {
    let ts = chrono::Utc::now().timestamp();
    let ts_hex = format!("{ts:x}");
    let mac = compute_mac(key, ts_hex.as_bytes());
    format!("{ts_hex}:{mac}")
}

fn verify_session_cookie(key: &[u8; 32], cookie_value: &str) -> bool {
    let parts: Vec<&str> = cookie_value.splitn(2, ':').collect();
    if parts.len() != 2 {
        return false;
    }
    let ts_hex = parts[0];
    let mac = parts[1];

    // Verify MAC
    let expected = compute_mac(key, ts_hex.as_bytes());
    if mac != expected {
        return false;
    }

    // Check expiry
    let ts = match i64::from_str_radix(ts_hex, 16) {
        Ok(t) => t,
        Err(_) => return false,
    };
    let now = chrono::Utc::now().timestamp();
    (now - ts) < SESSION_MAX_AGE_SECS
}

fn extract_session_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .find_map(|pair| {
            let pair = pair.trim();
            if let Some(val) = pair.strip_prefix(&format!("{SESSION_COOKIE_NAME}=")) {
                Some(val.to_string())
            } else {
                None
            }
        })
}

/// Auth middleware: redirects to login for pages, returns 401 for API calls.
async fn auth_middleware(
    State(state): State<AdminState>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let cookie = extract_session_cookie(request.headers());
    let authenticated = cookie
        .as_deref()
        .map(|c| verify_session_cookie(&state.signing_key, c))
        .unwrap_or(false);

    if !authenticated {
        // API endpoints get JSON 401; pages get redirect to login
        let path = request.uri().path().to_string();
        if path.contains("/admin/api/") {
            return (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({"error": "Not authenticated"})),
            )
                .into_response();
        }
        return Redirect::to("/admin/login").into_response();
    }

    next.run(request).await
}

/// Build the admin router.
pub fn build_admin_router(state: AdminState) -> Router {
    let protected = Router::new()
        .route("/admin", get(admin_dashboard))
        .route("/admin/api/config", get(api_config))
        .route("/admin/api/config/raw", get(api_config_raw).post(api_config_save))
        .route("/admin/api/miners", get(api_miners))
        .route("/admin/api/health", get(api_health))
        .route("/admin/api/payout/trigger", post(api_trigger_payout))
        .route("/admin/api/restart", post(api_restart))
        .route("/admin/api/miner/adjust", post(api_adjust_balance))
        .route("/admin/logout", post(handle_logout))
        .layer(middleware::from_fn_with_state(state.clone(), auth_middleware))
        .with_state(state.clone());

    let public = Router::new()
        .route("/admin/login", get(login_page).post(handle_login))
        .with_state(state);

    Router::new().merge(public).merge(protected)
}

// --- Handlers ---

#[derive(Deserialize)]
struct LoginForm {
    password: String,
}

async fn login_page() -> Html<&'static str> {
    Html(LOGIN_HTML)
}

async fn handle_login(
    State(state): State<AdminState>,
    Form(form): Form<LoginForm>,
) -> Response {
    // Verify password by deriving key and comparing
    let submitted_key = derive_signing_key(&form.password);
    if submitted_key != state.signing_key {
        return Html(LOGIN_FAIL_HTML).into_response();
    }

    let cookie = make_session_cookie(&state.signing_key);
    let set_cookie = format!(
        "{SESSION_COOKIE_NAME}={cookie}; Path=/admin; HttpOnly; SameSite=Lax; Secure; Max-Age={SESSION_MAX_AGE_SECS}"
    );
    (
        [(axum::http::header::SET_COOKIE, set_cookie)],
        Redirect::to("/admin"),
    )
        .into_response()
}

async fn handle_logout() -> Response {
    let clear = format!(
        "{SESSION_COOKIE_NAME}=; Path=/admin; HttpOnly; SameSite=Strict; Max-Age=0"
    );
    (
        [(axum::http::header::SET_COOKIE, clear)],
        Redirect::to("/admin/login"),
    )
        .into_response()
}

async fn admin_dashboard() -> Html<&'static str> {
    Html(ADMIN_DASHBOARD_HTML)
}

async fn api_config(State(state): State<AdminState>) -> Json<PoolConfigView> {
    Json(state.config_view.clone())
}

async fn api_config_raw(
    State(state): State<AdminState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    let content = tokio::fs::read_to_string(&state.config_path)
        .await
        .map_err(|e| (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to read config: {e}")})),
        ))?;
    Ok(Json(serde_json::json!({"content": content})))
}

#[derive(Deserialize)]
struct ConfigSaveRequest {
    content: String,
}

async fn api_config_save(
    State(state): State<AdminState>,
    Json(req): Json<ConfigSaveRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    // Validate it's parseable TOML before saving
    if let Err(e) = req.content.parse::<toml::Table>() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": format!("Invalid TOML: {e}")})),
        );
    }

    // Write to file
    if let Err(e) = tokio::fs::write(&state.config_path, &req.content).await {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Failed to write config: {e}")})),
        );
    }

    tracing::info!("Config file updated via admin panel");
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "message": "Config saved. Restart the pool to apply changes."
        })),
    )
}

#[derive(Serialize)]
struct AdminMinerInfo {
    id: i64,
    address: String,
    pending_zatoshis: i64,
    paid_zatoshis: i64,
    pending_zec: f64,
    paid_zec: f64,
    share_count: i64,
    worker_count: i64,
    last_seen: Option<String>,
    created_at: String,
}

async fn api_miners(
    State(state): State<AdminState>,
) -> Result<Json<Vec<AdminMinerInfo>>, StatusCode> {
    let miners = state
        .app
        .db
        .get_all_miners_admin()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    Ok(Json(
        miners
            .into_iter()
            .map(|m| AdminMinerInfo {
                id: m.id,
                address: m.address,
                pending_zatoshis: m.pending,
                paid_zatoshis: m.paid,
                pending_zec: m.pending as f64 / ZATOSHIS_PER_ZEC,
                paid_zec: m.paid as f64 / ZATOSHIS_PER_ZEC,
                share_count: m.share_count,
                worker_count: m.worker_count,
                last_seen: m.last_seen,
                created_at: m.created_at,
            })
            .collect(),
    ))
}

#[derive(Serialize)]
struct AdminHealth {
    node_ok: bool,
    node_height: Option<u64>,
    last_template_at: Option<String>,
    last_template_age_secs: Option<i64>,
    wallet_ok: bool,
    wallet_balance: Option<WalletBalanceInfo>,
    uptime_secs: i64,
    connected_miners: i64,
    connected_workers: i64,
    shares_accepted: u64,
    shares_rejected: u64,
    shares_rejection_rate: f64,
    system: Option<SystemStats>,
}

#[derive(Serialize)]
struct WalletBalanceInfo {
    transparent: String,
    private: String,
    total: String,
}

async fn api_health(
    State(state): State<AdminState>,
) -> Json<AdminHealth> {
    let now_ms = chrono::Utc::now().timestamp_millis();
    let now = chrono::Utc::now().timestamp();

    let (node_ok, last_template_at) = state.app.get_last_template_ms().await;
    let last_template_age_secs = last_template_at.as_ref().and_then(|ts| {
        chrono::DateTime::parse_from_rfc3339(ts).ok().map(|dt| {
            (now_ms - dt.timestamp_millis()) / 1000
        })
    });

    let node_height = tokio::time::timeout(
        std::time::Duration::from_secs(10),
        state.app.rpc.get_block_count(),
    ).await.ok().and_then(|r| r.ok());

    let (wallet_ok, wallet_balance) = match &state.app.wallet_rpc {
        Some(rpc) => {
            let fut = rpc.call_raw::<serde_json::Value>(
                "z_gettotalbalance",
                serde_json::json!([0, true]),
            );
            match tokio::time::timeout(std::time::Duration::from_secs(30), fut).await {
                Ok(Ok(v)) => {
                    let bal = v.as_object().map(|obj| WalletBalanceInfo {
                        transparent: obj.get("transparent").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                        private: obj.get("private").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                        total: obj.get("total").and_then(|v| v.as_str()).unwrap_or("?").to_string(),
                    });
                    (true, bal)
                }
                _ => (false, None),
            }
        }
        None => (false, None),
    };

    let connected_miners = state.app.db.get_connected_miners_count().await.unwrap_or(0);
    let connected_workers = state.app.db.get_connected_workers_count().await.unwrap_or(0);

    let (accepted, rejected) = state.app.get_shares_counters().await;
    let total = accepted + rejected;
    let rejection_rate = if total > 0 { (rejected as f64 / total as f64) * 100.0 } else { 0.0 };

    let system = read_system_stats();

    Json(AdminHealth {
        node_ok,
        node_height,
        last_template_at,
        last_template_age_secs,
        wallet_ok,
        wallet_balance,
        uptime_secs: now - state.started_at,
        connected_miners,
        connected_workers,
        shares_accepted: accepted,
        shares_rejected: rejected,
        shares_rejection_rate: rejection_rate,
        system,
    })
}

async fn api_restart() -> Json<serde_json::Value> {
    tracing::info!("Pool restart requested via admin panel");
    // Spawn a background process that restarts us after a short delay
    // so the HTTP response can be sent first.
    tokio::spawn(async {
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let _ = tokio::process::Command::new("sudo")
            .args(["systemctl", "restart", "zcash-pool"])
            .status()
            .await;
    });
    Json(serde_json::json!({"status": "ok", "message": "Restarting pool..."}))
}

async fn api_trigger_payout(
    State(state): State<AdminState>,
) -> (StatusCode, Json<serde_json::Value>) {
    crate::handlers::trigger_payout(State(state.app)).await
}

#[derive(Deserialize)]
struct AdjustBalanceRequest {
    address: String,
    amount_zatoshis: i64,
}

async fn api_adjust_balance(
    State(state): State<AdminState>,
    Json(req): Json<AdjustBalanceRequest>,
) -> (StatusCode, Json<serde_json::Value>) {
    let miner = match state.app.db.get_miner_by_address(&req.address).await {
        Ok(Some(m)) => m,
        Ok(None) => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({"error": "Miner not found"})),
            )
        }
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("DB error: {e}")})),
            )
        }
    };

    match state.app.db.adjust_miner_balance(miner.id, req.amount_zatoshis).await {
        Ok(new_balance) => (
            StatusCode::OK,
            Json(serde_json::json!({
                "status": "ok",
                "miner_id": miner.id,
                "address": req.address,
                "adjustment": req.amount_zatoshis,
                "new_pending_zatoshis": new_balance,
                "new_pending_zec": new_balance as f64 / ZATOSHIS_PER_ZEC,
            })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("DB error: {e}")})),
        ),
    }
}

// --- HTML ---

const LOGIN_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Admin Login - Zcash Mining Pool</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0a0e17; color: #e0e0e0; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
.login-box { background: #1a1f2e; border: 1px solid #2d3748; border-radius: 8px; padding: 2rem; width: 360px; }
.login-box h1 { color: #f4b728; font-size: 1.3rem; margin-bottom: 1.5rem; text-align: center; }
label { display: block; font-size: 0.8rem; color: #a0aec0; margin-bottom: 0.4rem; }
input[type="password"] { width: 100%; padding: 0.6rem 0.8rem; background: #0d1117; border: 1px solid #2d3748; border-radius: 4px; color: #e0e0e0; font-size: 0.95rem; margin-bottom: 1rem; }
input[type="password"]:focus { outline: none; border-color: #f4b728; }
button { width: 100%; padding: 0.6rem; background: #f4b728; color: #0a0e17; border: none; border-radius: 4px; font-weight: 600; font-size: 0.95rem; cursor: pointer; }
button:hover { background: #d69e2e; }
</style>
</head>
<body>
<div class="login-box">
<h1>Pool Admin</h1>
<form method="POST" action="/admin/login">
<label for="password">Password</label>
<input type="password" name="password" id="password" autofocus required>
<button type="submit">Login</button>
</form>
</div>
</body>
</html>
"##;

const LOGIN_FAIL_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Admin Login - Zcash Mining Pool</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0a0e17; color: #e0e0e0; min-height: 100vh; display: flex; align-items: center; justify-content: center; }
.login-box { background: #1a1f2e; border: 1px solid #2d3748; border-radius: 8px; padding: 2rem; width: 360px; }
.login-box h1 { color: #f4b728; font-size: 1.3rem; margin-bottom: 1.5rem; text-align: center; }
label { display: block; font-size: 0.8rem; color: #a0aec0; margin-bottom: 0.4rem; }
input[type="password"] { width: 100%; padding: 0.6rem 0.8rem; background: #0d1117; border: 1px solid #2d3748; border-radius: 4px; color: #e0e0e0; font-size: 0.95rem; margin-bottom: 1rem; }
input[type="password"]:focus { outline: none; border-color: #f4b728; }
button { width: 100%; padding: 0.6rem; background: #f4b728; color: #0a0e17; border: none; border-radius: 4px; font-weight: 600; font-size: 0.95rem; cursor: pointer; }
button:hover { background: #d69e2e; }
.error { color: #fc8181; font-size: 0.85rem; margin-bottom: 1rem; text-align: center; }
</style>
</head>
<body>
<div class="login-box">
<h1>Pool Admin</h1>
<div class="error">Invalid password</div>
<form method="POST" action="/admin/login">
<label for="password">Password</label>
<input type="password" name="password" id="password" autofocus required>
<button type="submit">Login</button>
</form>
</div>
</body>
</html>
"##;

const ADMIN_DASHBOARD_HTML: &str = r##"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>Admin - Zcash Mining Pool</title>
<style>
* { margin: 0; padding: 0; box-sizing: border-box; }
body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif; background: #0a0e17; color: #e0e0e0; min-height: 100vh; }
.header { background: linear-gradient(135deg, #1a1f2e 0%, #0d1117 100%); border-bottom: 1px solid #f4b728; padding: 1rem 2rem; display: flex; align-items: center; justify-content: space-between; flex-wrap: wrap; gap: 0.5rem; }
.header h1 { color: #f4b728; font-size: 1.3rem; }
.header-right { display: flex; align-items: center; gap: 1rem; }
.header-right a { color: #718096; text-decoration: none; font-size: 0.85rem; }
.header-right a:hover { color: #f4b728; }
.logout-btn { background: none; border: 1px solid #718096; color: #718096; padding: 0.3rem 0.8rem; border-radius: 4px; cursor: pointer; font-size: 0.8rem; }
.logout-btn:hover { border-color: #fc8181; color: #fc8181; }
.tabs { display: flex; gap: 0; background: #1a1f2e; border-bottom: 1px solid #2d3748; padding: 0 2rem; }
.tab { padding: 0.75rem 1.5rem; cursor: pointer; color: #718096; font-size: 0.9rem; border-bottom: 2px solid transparent; transition: all 0.15s; }
.tab:hover { color: #e0e0e0; }
.tab.active { color: #f4b728; border-bottom-color: #f4b728; }
.container { max-width: 1100px; margin: 0 auto; padding: 1.5rem 2rem; }
.panel { display: none; }
.panel.active { display: block; }
.card { background: #1a1f2e; border: 1px solid #2d3748; border-radius: 8px; padding: 1.25rem; margin-bottom: 1rem; }
.card h2 { font-size: 0.9rem; color: #a0aec0; margin-bottom: 0.75rem; padding-bottom: 0.5rem; border-bottom: 1px solid #2d3748; }
.kv-table { width: 100%; }
.kv-table td { padding: 0.35rem 0; font-size: 0.85rem; }
.kv-table td:first-child { color: #718096; width: 220px; }
.kv-table td:last-child { color: #e2e8f0; font-family: 'JetBrains Mono', 'Fira Code', monospace; word-break: break-all; }
table.data { width: 100%; border-collapse: collapse; }
table.data th { font-size: 0.65rem; text-transform: uppercase; letter-spacing: 0.08em; color: #718096; padding: 0.5rem; text-align: left; border-bottom: 1px solid #2d3748; }
table.data td { font-size: 0.8rem; padding: 0.5rem; border-bottom: 1px solid #1a2332; color: #a0aec0; font-family: 'JetBrains Mono', 'Fira Code', monospace; }
table.data tr:hover { background: rgba(244, 183, 40, 0.03); }
.badge { padding: 0.2rem 0.6rem; border-radius: 4px; font-size: 0.75rem; font-weight: 600; }
.badge-ok { background: #22543d; color: #68d391; }
.badge-fail { background: #742a2a; color: #fc8181; }
.btn { padding: 0.4rem 1rem; border: none; border-radius: 4px; font-weight: 600; font-size: 0.8rem; cursor: pointer; }
.btn-primary { background: #f4b728; color: #0a0e17; }
.btn-primary:hover { background: #d69e2e; }
.btn-primary:disabled { opacity: 0.5; cursor: not-allowed; }
.btn-sm { padding: 0.25rem 0.6rem; font-size: 0.75rem; }
.btn-danger { background: #742a2a; color: #fc8181; }
.btn-danger:hover { background: #9b2c2c; }
.status-msg { margin-top: 0.75rem; padding: 0.5rem 0.75rem; border-radius: 4px; font-size: 0.8rem; display: none; }
.status-msg.ok { display: block; background: #22543d; color: #68d391; }
.status-msg.err { display: block; background: #742a2a; color: #fc8181; }
.search-box { width: 100%; padding: 0.5rem 0.75rem; background: #0d1117; border: 1px solid #2d3748; border-radius: 4px; color: #e0e0e0; font-size: 0.85rem; margin-bottom: 0.75rem; }
.search-box:focus { outline: none; border-color: #f4b728; }
.adjust-input { width: 100px; padding: 0.2rem 0.4rem; background: #0d1117; border: 1px solid #2d3748; border-radius: 4px; color: #e0e0e0; font-size: 0.75rem; font-family: monospace; }
.mono { font-family: 'JetBrains Mono', 'Fira Code', monospace; }
.addr { max-width: 180px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
.gold { color: #f4b728; }
.loading { color: #4a5568; font-size: 0.8rem; padding: 1rem; text-align: center; }
</style>
</head>
<body>
<div class="header">
    <h1>Pool Admin</h1>
    <div class="header-right">
        <a href="/" target="_blank">Public Dashboard</a>
        <form method="POST" action="/admin/logout" style="display:inline"><button type="submit" class="logout-btn">Logout</button></form>
    </div>
</div>
<div class="tabs">
    <div class="tab active" data-tab="config">Config</div>
    <div class="tab" data-tab="payouts">Payouts</div>
    <div class="tab" data-tab="miners">Miners</div>
    <div class="tab" data-tab="health">Health</div>
</div>
<div class="container">

<!-- Config Tab -->
<div class="panel active" id="panel-config">
    <div class="card">
        <h2>Running Configuration (read-only snapshot)</h2>
        <div id="config-content" class="loading">Loading...</div>
    </div>
    <div class="card">
        <h2>Edit pool.toml</h2>
        <textarea id="config-editor" spellcheck="false" style="width:100%;height:420px;background:#0d1117;color:#e2e8f0;border:1px solid #2d3748;border-radius:4px;padding:0.75rem;font-family:'JetBrains Mono','Fira Code',monospace;font-size:0.8rem;resize:vertical;line-height:1.5;tab-size:4"></textarea>
        <div style="margin-top:0.75rem;display:flex;align-items:center;gap:1rem">
            <button class="btn btn-primary" onclick="saveConfig()">Save</button>
            <span id="config-save-status" style="font-size:0.8rem"></span>
        </div>
        <p style="font-size:0.7rem;color:#718096;margin-top:0.5rem">Changes are saved to disk. Restart the pool service to apply.</p>
    </div>
    <div class="card">
        <h2>Restart Pool</h2>
        <p style="font-size:0.8rem;color:#718096;margin-bottom:0.75rem">Restart the pool process to apply config changes. This will briefly disconnect all miners.</p>
        <button class="btn btn-danger" onclick="restartPool()">Restart Pool</button>
        <span id="restart-status" style="font-size:0.8rem;margin-left:1rem"></span>
    </div>
</div>

<!-- Payouts Tab -->
<div class="panel" id="panel-payouts">
    <div class="card">
        <h2>Trigger Manual Payout</h2>
        <p style="font-size:0.8rem;color:#718096;margin-bottom:0.75rem">Runs the full payout pipeline: check maturity, shield coinbase, send payouts.</p>
        <button class="btn btn-primary" id="btn-payout" onclick="triggerPayout()">Trigger Payout</button>
        <div id="payout-status" class="status-msg"></div>
    </div>
    <div class="card">
        <h2>Wallet Balances</h2>
        <div id="wallet-bal" class="loading">Loading...</div>
    </div>
    <div class="card">
        <h2>Immature Blocks</h2>
        <div id="immature-content" class="loading">Loading...</div>
    </div>
</div>

<!-- Miners Tab -->
<div class="panel" id="panel-miners">
    <div class="card">
        <h2>All Miners</h2>
        <input type="text" class="search-box" id="miner-search" placeholder="Search by address..." oninput="filterMiners()">
        <div id="miners-content" class="loading">Loading...</div>
    </div>
</div>

<!-- Health Tab -->
<div class="panel" id="panel-health">
    <div class="card">
        <h2>System Health</h2>
        <div id="health-content" class="loading">Loading...</div>
    </div>
</div>

</div>
<script>
// Tab switching
let currentTab = 'config';
document.querySelectorAll('.tab').forEach(tab => {
    tab.addEventListener('click', () => {
        document.querySelectorAll('.tab').forEach(t => t.classList.remove('active'));
        document.querySelectorAll('.panel').forEach(p => p.classList.remove('active'));
        tab.classList.add('active');
        const id = tab.dataset.tab;
        document.getElementById('panel-' + id).classList.add('active');
        currentTab = id;
        refreshTab(id);
    });
});

let allMiners = [];

// Safe JSON fetch: returns null on non-JSON responses (502, redirects, etc.)
async function fetchJson(url, opts) {
    const r = await fetch(url, opts || {});
    if (r.status === 401) { window.location.href = '/admin/login'; return null; }
    const ct = r.headers.get('content-type') || '';
    if (!ct.includes('application/json')) return null;
    return r.json();
}

function refreshTab(tab) {
    if (tab === 'config') fetchConfig();
    else if (tab === 'payouts') { fetchWalletBal(); fetchImmature(); }
    else if (tab === 'miners') fetchMiners();
    else if (tab === 'health') fetchHealth();
}

async function fetchConfig() {
    try {
        const d = await fetchJson('/admin/api/config');
        if (!d) return;
        let html = '<table class="kv-table">';
        const rows = [
            ['Pool Name', d.pool_name],
            ['Network', d.network],
            ['Fee', d.pool_fee + '%'],
            ['Stratum Ports', d.stratum_ports.join(', ')],
            ['Difficulty Multiplier', d.difficulty_multiplier.toFixed(2)],
            ['Min Payout', d.min_payout_zec + ' ZEC'],
            ['Maturity Confirmations', d.maturity_confirmations],
            ['Payout Interval', d.payout_interval_secs + 's'],
            ['Pool Address', d.pool_address || 'N/A'],
            ['Mining Address', d.mining_address || 'N/A'],
            ['Node RPC', d.node_rpc_url],
            ['Wallet RPC', d.wallet_rpc_url || 'N/A'],
            ['Coinbase Tag', d.coinbase_tag || 'N/A'],
        ];
        for (const [k, v] of rows) {
            html += '<tr><td>' + k + '</td><td>' + v + '</td></tr>';
        }
        html += '</table>';
        document.getElementById('config-content').innerHTML = html;
    } catch (e) {
        document.getElementById('config-content').innerHTML = '<span style="color:#fc8181">Failed to load: ' + e + '</span>';
    }
    // Load raw TOML into editor
    try {
        const d = await fetchJson('/admin/api/config/raw');
        const editor = document.getElementById('config-editor');
        if (editor && d && d.content) editor.value = d.content;
    } catch (e) {}
}

async function saveConfig() {
    const content = document.getElementById('config-editor').value;
    const el = document.getElementById('config-save-status');
    el.textContent = 'Saving...';
    el.style.color = '#718096';
    try {
        const d = await fetchJson('/admin/api/config/raw', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ content }),
        });
        if (d && !d.error) {
            el.textContent = 'Saved. Restart pool to apply.';
            el.style.color = '#68d391';
        } else {
            el.textContent = (d && d.error) || 'Save failed';
            el.style.color = '#fc8181';
        }
    } catch (e) {
        el.textContent = 'Request failed: ' + e;
        el.style.color = '#fc8181';
    }
}

async function restartPool() {
    if (!confirm('Restart the pool? All miners will be briefly disconnected.')) return;
    const el = document.getElementById('restart-status');
    el.textContent = 'Restarting...';
    el.style.color = '#f4b728';
    try {
        await fetch('/admin/api/restart', { method: 'POST' });
        el.textContent = 'Restart signal sent. Page will reload...';
        el.style.color = '#68d391';
        setTimeout(() => location.reload(), 5000);
    } catch (e) {
        el.textContent = 'Failed: ' + e;
        el.style.color = '#fc8181';
    }
}

async function fetchWalletBal() {
    try {
        const d = await fetchJson('/admin/api/health');
        if (!d) return;
        if (d.wallet_balance) {
            const b = d.wallet_balance;
            document.getElementById('wallet-bal').innerHTML =
                '<table class="kv-table">' +
                '<tr><td>Transparent</td><td class="gold">' + b.transparent + ' ZEC</td></tr>' +
                '<tr><td>Private (Shielded)</td><td class="gold">' + b.private + ' ZEC</td></tr>' +
                '<tr><td>Total</td><td class="gold">' + b.total + ' ZEC</td></tr>' +
                '</table>';
        } else {
            document.getElementById('wallet-bal').innerHTML = '<span class="badge badge-fail">Wallet offline</span>';
        }
    } catch (e) {
        document.getElementById('wallet-bal').innerHTML = '<span style="color:#fc8181">Error: ' + e + '</span>';
    }
}

async function fetchImmature() {
    try {
        const h = await fetchJson('/admin/api/health');
        if (!h) return;
        document.getElementById('immature-content').innerHTML =
            '<p style="font-size:0.85rem;color:#a0aec0">See the public dashboard for detailed immature block info. Node height: <span class="gold">' + (h.node_height || '?') + '</span></p>';
    } catch (e) {
        document.getElementById('immature-content').innerHTML = '<span style="color:#fc8181">Error</span>';
    }
}

async function triggerPayout() {
    const btn = document.getElementById('btn-payout');
    const el = document.getElementById('payout-status');
    btn.disabled = true;
    btn.textContent = 'Running...';
    el.className = 'status-msg';
    el.style.display = 'none';
    try {
        const d = await fetchJson('/admin/api/payout/trigger', { method: 'POST' });
        if (d && !d.error) {
            el.className = 'status-msg ok';
            el.textContent = JSON.stringify(d, null, 2);
            el.style.display = 'block';
            el.style.whiteSpace = 'pre';
            el.style.fontFamily = 'monospace';
            el.style.fontSize = '0.8rem';
        } else {
            el.className = 'status-msg err';
            el.textContent = d.message || JSON.stringify(d);
            el.style.display = 'block';
        }
    } catch (e) {
        el.className = 'status-msg err';
        el.textContent = 'Request failed: ' + e;
        el.style.display = 'block';
    }
    btn.disabled = false;
    btn.textContent = 'Trigger Payout';
}

async function fetchMiners() {
    try {
        const d = await fetchJson('/admin/api/miners');
        if (!d) return;
        allMiners = d;
        renderMiners(allMiners);
    } catch (e) {
        document.getElementById('miners-content').innerHTML = '<span style="color:#fc8181">Failed: ' + e + '</span>';
    }
}

function filterMiners() {
    const q = document.getElementById('miner-search').value.toLowerCase();
    const filtered = q ? allMiners.filter(m => m.address.toLowerCase().includes(q)) : allMiners;
    renderMiners(filtered);
}

function renderMiners(miners) {
    if (!miners.length) {
        document.getElementById('miners-content').innerHTML = '<div class="loading">No miners found</div>';
        return;
    }
    let html = '<table class="data"><thead><tr><th>ID</th><th>Address</th><th>Pending</th><th>Paid</th><th>Shares</th><th>Workers</th><th>Last Seen</th><th>Adjust</th></tr></thead><tbody>';
    for (const m of miners) {
        const short = m.address.length > 20 ? m.address.slice(0, 10) + '...' + m.address.slice(-8) : m.address;
        const ls = m.last_seen || 'never';
        html += '<tr>' +
            '<td>' + m.id + '</td>' +
            '<td title="' + m.address + '" class="addr">' + short + '</td>' +
            '<td class="gold">' + m.pending_zec.toFixed(4) + '</td>' +
            '<td>' + m.paid_zec.toFixed(4) + '</td>' +
            '<td>' + m.share_count + '</td>' +
            '<td>' + m.worker_count + '</td>' +
            '<td>' + ls + '</td>' +
            '<td><input class="adjust-input" type="number" id="adj-' + m.id + '" placeholder="zatoshis">' +
            ' <button class="btn btn-sm btn-primary" onclick="adjustBalance(\'' + m.address + '\',' + m.id + ')">Set</button></td>' +
            '</tr>';
    }
    html += '</tbody></table>';
    html += '<p style="font-size:0.7rem;color:#718096;margin-top:0.5rem">Adjust: enter positive to add, negative to subtract zatoshis from pending balance.</p>';
    document.getElementById('miners-content').innerHTML = html;
}

async function adjustBalance(address, minerId) {
    const input = document.getElementById('adj-' + minerId);
    const val = parseInt(input.value, 10);
    if (isNaN(val) || val === 0) { alert('Enter a non-zero amount'); return; }
    if (!confirm('Adjust ' + address.slice(0, 12) + '... by ' + val + ' zatoshis?')) return;
    try {
        const d = await fetchJson('/admin/api/miner/adjust', {
            method: 'POST',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ address: address, amount_zatoshis: val }),
        });
        if (d && d.status === 'ok') {
            input.value = '';
            fetchMiners();
        } else {
            alert('Error: ' + (d.error || JSON.stringify(d)));
        }
    } catch (e) {
        alert('Request failed: ' + e);
    }
}

function fmtDuration(secs) {
    const d = Math.floor(secs / 86400);
    const h = Math.floor((secs % 86400) / 3600);
    const m = Math.floor((secs % 3600) / 60);
    let s = '';
    if (d > 0) s += d + 'd ';
    if (h > 0) s += h + 'h ';
    s += m + 'm';
    return s;
}

async function fetchHealth() {
    try {
        const d = await fetchJson('/admin/api/health');
        if (!d) return;
        let html = '<table class="kv-table">';
        html += '<tr><td>Zebrad (Node)</td><td>' + (d.node_ok ? '<span class="badge badge-ok">Online</span>' : '<span class="badge badge-fail">Offline/Stalled</span>') + '</td></tr>';
        html += '<tr><td>Node Height</td><td>' + (d.node_height || '?') + '</td></tr>';
        html += '<tr><td>Last Template</td><td>' + (d.last_template_at || 'N/A') + (d.last_template_age_secs != null ? ' (' + d.last_template_age_secs + 's ago)' : '') + '</td></tr>';
        html += '<tr><td>Zallet (Wallet)</td><td>' + (d.wallet_ok ? '<span class="badge badge-ok">Online</span>' : '<span class="badge badge-fail">Offline</span>') + '</td></tr>';
        if (d.wallet_balance) {
            html += '<tr><td>Wallet Total</td><td class="gold">' + d.wallet_balance.total + ' ZEC</td></tr>';
        }
        html += '<tr><td>Uptime</td><td>' + fmtDuration(d.uptime_secs) + '</td></tr>';
        html += '<tr><td>Connected Miners</td><td>' + d.connected_miners + '</td></tr>';
        html += '<tr><td>Connected Workers</td><td>' + d.connected_workers + '</td></tr>';
        html += '<tr><td>Shares Accepted</td><td>' + d.shares_accepted.toLocaleString() + '</td></tr>';
        html += '<tr><td>Shares Rejected</td><td>' + d.shares_rejected.toLocaleString() + '</td></tr>';
        const rateColor = d.shares_rejection_rate > 5 ? '#fc8181' : d.shares_rejection_rate > 1 ? '#f4b728' : '#68d391';
        html += '<tr><td>Rejection Rate</td><td style="color:' + rateColor + '">' + d.shares_rejection_rate.toFixed(2) + '%</td></tr>';
        html += '</table>';

        // System stats (Linux only)
        if (d.system) {
            const s = d.system;
            html += '<h2 style="font-size:0.9rem;color:#a0aec0;margin:1.25rem 0 0.75rem;padding-bottom:0.5rem;border-bottom:1px solid #2d3748">System Resources</h2>';
            html += '<table class="kv-table">';

            // Load average
            const loadColor = (v) => v >= s.cpu_count * 2 ? '#fc8181' : v >= s.cpu_count ? '#f4b728' : '#68d391';
            html += '<tr><td>Load Average</td><td>' +
                '<span style="color:' + loadColor(s.load_1m) + '">' + s.load_1m.toFixed(2) + '</span> / ' +
                '<span style="color:' + loadColor(s.load_5m) + '">' + s.load_5m.toFixed(2) + '</span> / ' +
                '<span style="color:' + loadColor(s.load_15m) + '">' + s.load_15m.toFixed(2) + '</span>' +
                ' <span style="color:#718096">(' + s.cpu_count + ' CPUs)</span></td></tr>';

            // Memory
            const memColor = s.mem_percent >= 95 ? '#fc8181' : s.mem_percent >= 80 ? '#f4b728' : '#68d391';
            html += '<tr><td>Memory</td><td style="color:' + memColor + '">' +
                s.mem_used_mb.toLocaleString() + ' / ' + s.mem_total_mb.toLocaleString() + ' MB (' + s.mem_percent.toFixed(1) + '%)</td></tr>';

            // Swap
            if (s.swap_total_mb > 0) {
                const swapPct = s.swap_used_mb / s.swap_total_mb * 100;
                const swapColor = swapPct >= 80 ? '#fc8181' : swapPct >= 50 ? '#f4b728' : '#68d391';
                html += '<tr><td>Swap</td><td style="color:' + swapColor + '">' +
                    s.swap_used_mb.toLocaleString() + ' / ' + s.swap_total_mb.toLocaleString() + ' MB (' + swapPct.toFixed(1) + '%)</td></tr>';
            }

            // Disk
            const diskColor = s.disk_percent >= 95 ? '#fc8181' : s.disk_percent >= 80 ? '#f4b728' : '#68d391';
            html += '<tr><td>Disk</td><td style="color:' + diskColor + '">' +
                s.disk_used_gb.toFixed(1) + ' / ' + s.disk_total_gb.toFixed(1) + ' GB (' + s.disk_percent.toFixed(1) + '%)</td></tr>';

            // DB size
            const dbColor = s.db_size_mb >= 1024 ? '#fc8181' : s.db_size_mb >= 500 ? '#f4b728' : '#68d391';
            html += '<tr><td>DB Size (pool.db)</td><td style="color:' + dbColor + '">' + s.db_size_mb.toFixed(1) + ' MB</td></tr>';

            // File descriptors
            const fdPct = s.fd_limit > 0 ? s.open_fds / s.fd_limit * 100 : 0;
            const fdColor = fdPct >= 80 ? '#fc8181' : fdPct >= 50 ? '#f4b728' : '#68d391';
            html += '<tr><td>File Descriptors</td><td style="color:' + fdColor + '">' +
                s.open_fds.toLocaleString() + ' / ' + s.fd_limit.toLocaleString() + (s.fd_limit > 0 ? ' (' + fdPct.toFixed(1) + '%)' : '') + '</td></tr>';

            html += '</table>';
        }

        document.getElementById('health-content').innerHTML = html;
    } catch (e) {
        document.getElementById('health-content').innerHTML = '<span style="color:#fc8181">Failed: ' + e + '</span>';
    }
}

// Initial load + auto-refresh
fetchConfig();
setInterval(() => { if (currentTab === 'health') fetchHealth(); }, 10000);
setInterval(() => { if (currentTab === 'payouts') { fetchWalletBal(); } }, 15000);
setInterval(() => { if (currentTab === 'miners') fetchMiners(); }, 30000);
</script>
</body>
</html>
"##;
