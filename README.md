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
