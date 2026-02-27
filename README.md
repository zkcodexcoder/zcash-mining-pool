# Zcash Mining Pool

A Rust-based mining pool for Zcash testnet, built with Zebrad + Zallet.

## Architecture

```
Miners ──stratum──► zcash-pool ──rpc──► zebrad (remote)
                        │
                        └──rpc──► zallet (local wallet)
```

**Crates:** `node-rpc`, `stratum`, `pool-core`, `pool-db` (SQLite), `pool-api` (Axum), `pool-server`, `rewards` (PPLNS), `cpu-miner`

## Building

```bash
cargo build --release
```

Binary: `target/release/zcash-pool`

## Configuration

Edit `config/pool.toml` — see the file for all options (stratum port, node RPC, wallet RPC, payout settings, etc.)

## Running

### Manual

```bash
cd /path/to/zcash-mining-pool
./target/release/zcash-pool config/pool.toml
```

The working directory **must** be the project root (the SQLite DB path is relative).

### Systemd (Production)

Service files are in `systemd/`. The pool depends on Zallet, so Zallet starts first.

#### Install services

```bash
# Copy service files (requires sudo)
sudo cp systemd/zallet.service /etc/systemd/system/
sudo cp systemd/zcash-pool.service /etc/systemd/system/

# Reload systemd
sudo systemctl daemon-reload

# Enable services to start on boot
sudo systemctl enable zallet
sudo systemctl enable zcash-pool
```

#### Start / Stop / Restart

```bash
# Start both (pool auto-starts with zallet due to Requires=)
sudo systemctl start zallet
sudo systemctl start zcash-pool

# Restart just the pool (zallet stays up)
sudo systemctl restart zcash-pool

# Restart both
sudo systemctl restart zallet   # pool auto-restarts due to Requires=

# Stop both
sudo systemctl stop zcash-pool
sudo systemctl stop zallet
```

#### Check status

```bash
sudo systemctl status zallet
sudo systemctl status zcash-pool
```

#### View logs

```bash
# Systemd journal
journalctl -u zcash-pool -f          # follow pool logs
journalctl -u zallet -f              # follow zallet logs
journalctl -u zcash-pool --since "1 hour ago"

# Log files (services also append to these)
tail -f ~/zcash-mining-pool/pool.log
tail -f ~/zallet.log
```

#### After rebuilding

```bash
cd ~/zcash-mining-pool && git pull
cargo build --release
sudo systemctl restart zcash-pool
```

#### First-time migration from nohup

If processes are currently running via nohup, stop them first:

```bash
# Kill existing nohup processes
pkill -f zcash-pool
pkill -f start-zallet.sh
pkill -f 'zallet.*start'

# Install and start services
sudo cp systemd/zallet.service /etc/systemd/system/
sudo cp systemd/zcash-pool.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable --now zallet
sudo systemctl enable --now zcash-pool
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
| `stratum+tcp://HOST:3333` | Stratum mining |

## Server

- **Host:** `operational-host.invalid`
- **Dashboard:** http://operational-host.invalid:8080
- **Stratum:** `operational-host.invalid:3333`
- **Zebrad:** `operational-host.invalid:18232` (remote)
- **Zallet RPC:** `127.0.0.1:28232` (local)
