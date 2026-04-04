# Zcash Mining Pool

A Rust-based mining pool for Zcash testnet, built with Zebrad + Zallet.

## Architecture

```
Miners ──stratum──► zcash-pool ──writes──► pool.db (SQLite WAL)
                        │
                        └──rpc──► zebrad (remote)

Browser ──http──► zcash-dashboard ──reads──► pool.db + zebrad RPC + zallet RPC
```

Two independent binaries share the same SQLite database (WAL mode):

- **`zcash-pool`** — Mining only: stratum server, job manager, share validator, payouts. Writes live stats to `pool_status` table every 5s.
- **`zcash-dashboard`** — Dashboard, API, and admin panel. Reads from the same DB. Can be restarted without disconnecting miners.

**Crates:** `node-rpc`, `stratum`, `pool-core`, `pool-db` (SQLite), `pool-api` (Axum), `pool-server`, `pool-dashboard`, `rewards` (PPLNS), `cpu-miner`

## Building

```bash
cargo build --release
```

Binaries: `target/release/zcash-pool` and `target/release/zcash-dashboard`

## Configuration

Edit `config/pool.toml` — see `config/pool.toml.example` for all options (stratum port, node RPC, wallet RPC, payout settings, etc.)

Both binaries read the same `pool.toml`. The dashboard ignores mining-specific sections (stratum, pplns).

## Running

### Manual

```bash
cd /path/to/zcash-mining-pool

# Start mining server
./target/release/zcash-pool config/pool.toml

# Start dashboard (in another terminal)
./target/release/zcash-dashboard config/pool.toml

# Dashboard with port override (for side-by-side testing)
./target/release/zcash-dashboard config/pool.toml --port 8081
```

The working directory **must** be the project root (the SQLite DB path is relative).

### Systemd (Production)

Service files are in `systemd/`. The pool depends on Zallet; the dashboard is independent.

#### Install services

```bash
sudo cp systemd/zallet.service /etc/systemd/system/
sudo cp systemd/zcash-pool.service /etc/systemd/system/
sudo cp systemd/zcash-dashboard.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable zallet zcash-pool zcash-dashboard
```

#### Start / Stop / Restart

```bash
# Start all
sudo systemctl start zallet
sudo systemctl start zcash-pool
sudo systemctl start zcash-dashboard

# Dashboard changes only (zero miner impact)
sudo systemctl restart zcash-dashboard

# Mining changes (miners reconnect briefly)
sudo systemctl restart zcash-pool

# Restart everything
sudo systemctl restart zallet   # pool auto-restarts due to Requires=
sudo systemctl restart zcash-dashboard
```

#### Check status

```bash
sudo systemctl status zallet
sudo systemctl status zcash-pool
sudo systemctl status zcash-dashboard
```

#### View logs

```bash
# Log files
tail -f ~/zcash-mining-pool/pool.log
tail -f ~/zcash-mining-pool/dashboard.log

# Systemd journal
journalctl -u zcash-pool -f
journalctl -u zcash-dashboard -f
journalctl -u zallet -f
```

#### Deploy workflow

```bash
# Local: commit & push
# Server:
cd ~/zcash-mining-pool && git pull
source ~/.cargo/env && cargo build --release

# Dashboard changes only (zero miner impact):
sudo systemctl restart zcash-dashboard

# Mining changes (miners reconnect):
sudo systemctl restart zcash-pool

# Both:
sudo systemctl restart zcash-pool zcash-dashboard
```

## Upgrading

### From single-binary to two-binary architecture

Older versions ran everything in a single `zcash-pool` binary (stratum + API + dashboard + admin). The new architecture splits this into `zcash-pool` (mining only) and `zcash-dashboard` (everything else). This means dashboard deploys no longer disconnect miners.

**Upgrade steps on an existing server:**

```bash
# 1. Pull latest code and build both binaries
cd ~/zcash-mining-pool && git pull
source ~/.cargo/env && cargo build --release

# 2. Install the new dashboard service file
sudo cp systemd/zcash-dashboard.service /etc/systemd/system/
sudo systemctl daemon-reload

# 3. Also update the pool service file if it's stale
sudo cp systemd/zcash-pool.service /etc/systemd/system/
sudo systemctl daemon-reload

# 4. Restart the pool (picks up mining-only binary, frees port 8080)
#    Miners will reconnect automatically within seconds.
sudo systemctl restart zcash-pool

# 5. Start the dashboard (takes over port 8080)
sudo systemctl enable --now zcash-dashboard

# 6. Verify
systemctl status zcash-pool        # should be active (running)
systemctl status zcash-dashboard   # should be active (running)
curl -s http://localhost:8080/health   # should return {"status":"ok"}
```

**What changes:**
- `zcash-pool` no longer serves HTTP — no API, no dashboard, no admin panel
- `zcash-pool` writes `last_template_at_ms`, `shares_accepted`, `shares_rejected` to a `pool_status` SQLite table every 5s
- `zcash-dashboard` reads from the same database (WAL mode enables concurrent access) and serves the dashboard/API/admin
- The database auto-migrates: a new `pool_status` table is created on first run
- Both binaries read the same `config/pool.toml` — no config changes needed

**Rollback:** If something goes wrong, you can revert to the old single-binary by checking out the previous commit and rebuilding. The `pool_status` table is harmless and will be ignored by the old binary.

### Testnet to Mainnet

Edit `config/pool.toml`:

```toml
[pool]
network = "mainnet"

[node]
rpc_url = "http://<mainnet-zebrad-ip>:8232"

[payout]
pool_address = "u1..."          # mainnet unified address
mining_address = "t1..."        # mainnet transparent address
wallet_rpc_url = "http://127.0.0.1:8232"
wallet_rpc_user = "<user>"
wallet_rpc_password = "<password>"
```

Also update `config/zebrad.toml`: `network = "Mainnet"`, ports `8232`/`8233`.

Then rebuild and restart:

```bash
cargo build --release
sudo systemctl restart zcash-pool zcash-dashboard
```

The `network` setting controls address validation prefixes, currency symbol (TAZ/ZEC), and badge text on the dashboard.

## Endpoints

| Endpoint | Description |
|----------|-------------|
| `http://HOST:8080` | Dashboard |
| `http://HOST:8080/network` | Network mining stats |
| `http://HOST:8080/zallet` | Wallet status |
| `http://HOST:8080/api/pool/stats` | Pool stats JSON |
| `http://HOST:8080/api/pool/stats/history` | Stats history (1hr ring buffer) |
| `http://HOST:8080/health` | Health check |
| `http://HOST:9091/admin` | Admin panel (password-protected) |
| `stratum+tcp://HOST:3333` | Stratum mining |

## Server

- **Host:** `operational-host.invalid`
- **Dashboard:** http://operational-host.invalid:8080
- **Stratum:** `operational-host.invalid:3333`
- **Zebrad:** `operational-host.invalid:18232` (remote)
- **Zallet RPC:** `127.0.0.1:28232` (local)
