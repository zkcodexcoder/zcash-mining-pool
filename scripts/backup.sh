#!/usr/bin/env bash
# Nightly pool backups (audit #18 — there were NONE before 2026-08-14).
# Safe while services run: sqlite3 .backup takes a consistent snapshot via the
# backup API (works with WAL); no service stops, no locks held long.
# Cron: 30 3 * * *  /home/zebra/zecminer/pool/scripts/backup.sh >> /home/zebra/backups/backup.log 2>&1
set -euo pipefail
TS=$(date -u +%Y%m%d)
DEST=/home/zebra/backups
POOL_DB=/home/zebra/zecminer/pool/pool.db
WALLET_DB=/home/zebra/.zallet-beta1/wallet.db

echo "[$(date -u +%FT%TZ)] backup start"

# 1. pool.db — consistent snapshot, then compress.
sqlite3 "$POOL_DB" ".backup '$DEST/pool-$TS.db'"
gzip -f "$DEST/pool-$TS.db"

# 2. Payout wallet — zecd since 2026-09-12 (audit B22). zecd's datadir is a
#    disposable cache: everything rebuilds from keys.toml (the age-encrypted seed
#    plus birthday) and its config (network, backend, RPC credentials). The age
#    identity that decrypts keys.toml IS spend authority, so it is deliberately
#    NOT copied here — a second copy on the same disk adds no protection. Keep it,
#    and the offline mnemonic, off this machine.
ZECD_DIR=/home/zebra/zecd-main
if [ -f "$ZECD_DIR/data/payout/keys.toml" ]; then
  install -m 600 "$ZECD_DIR/data/payout/keys.toml" "$DEST/zecd-keys.toml"
  install -m 600 "$ZECD_DIR/zecd.toml" "$DEST/zecd-config.toml"
else
  echo "[$(date -u +%FT%TZ)] WARN zecd keys.toml not found at $ZECD_DIR"
fi

# 3. zallet (retired 2026-09-12, kept installed for rollback) — best effort only,
#    so a missing or locked zallet can never abort the rest of the backup.
if [ -f "$WALLET_DB" ]; then
  if sqlite3 "file:$WALLET_DB?mode=ro" ".backup '$DEST/wallet-$TS.db'"; then
    gzip -f "$DEST/wallet-$TS.db"
  else
    echo "[$(date -u +%FT%TZ)] WARN zallet wallet.db snapshot failed (retired wallet)"
  fi
fi
cp -f /home/zebra/.zallet-beta1/encryption-identity.txt "$DEST/encryption-identity.txt" 2>/dev/null || true
cp -f /home/zebra/.zallet-beta1/zallet.toml "$DEST/zallet-config.toml" 2>/dev/null || true

# 4. pool config (small, versionless — git-ignored on purpose).
cp -f /home/zebra/zecminer/pool/config/pool.toml "$DEST/pool-config-$TS.toml"

# 5. Git bundle — full repo history in the backup set. GitHub (zkcodexcoder)
#    has been pushable since 2026-08-21; the bundle stays as cheap redundancy
#    (it also captures unpushed local work).
git -C /home/zebra/zecminer/pool bundle create "$DEST/pool-repo-$TS.bundle" --all -q 2>/dev/null || true

# 6. Rotate: keep 7 days of dated files.
find "$DEST" -name 'pool-*.db.gz'         -mtime +7 -delete
find "$DEST" -name 'wallet-*.db.gz'       -mtime +7 -delete
find "$DEST" -name 'pool-config-*.toml'   -mtime +7 -delete
find "$DEST" -name 'pool-repo-*.bundle'   -mtime +7 -delete

echo "[$(date -u +%FT%TZ)] backup done: $(ls -lh $DEST/pool-$TS.db.gz $DEST/wallet-$TS.db.gz 2>/dev/null | awk '{print $5, $9}' | tr '\n' ' ')"
