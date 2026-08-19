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

# 2. zallet wallet.db — same approach (sqlite under the hood). Keys live in
#    the encrypted keystore inside the same datadir; snapshot both.
sqlite3 "file:$WALLET_DB?mode=ro" ".backup '$DEST/wallet-$TS.db'"
gzip -f "$DEST/wallet-$TS.db"
cp -f /home/zebra/.zallet-beta1/encryption-identity.txt "$DEST/encryption-identity.txt" 2>/dev/null || true
cp -f /home/zebra/.zallet-beta1/zallet.toml "$DEST/zallet-config.toml" 2>/dev/null || true

# 3. pool config (small, versionless — git-ignored on purpose).
cp -f /home/zebra/zecminer/pool/config/pool.toml "$DEST/pool-config-$TS.toml"

# 4. Git bundle — full repo history in the backup set. GitHub push has been
#    broken since the account suspension (2026-07-01), so until a remote works
#    again this is the only off-disk copy of the code history.
git -C /home/zebra/zecminer/pool bundle create "$DEST/pool-repo-$TS.bundle" --all -q 2>/dev/null || true

# 5. Rotate: keep 7 days of dated files.
find "$DEST" -name 'pool-*.db.gz'         -mtime +7 -delete
find "$DEST" -name 'wallet-*.db.gz'       -mtime +7 -delete
find "$DEST" -name 'pool-config-*.toml'   -mtime +7 -delete
find "$DEST" -name 'pool-repo-*.bundle'   -mtime +7 -delete

echo "[$(date -u +%FT%TZ)] backup done: $(ls -lh $DEST/pool-$TS.db.gz $DEST/wallet-$TS.db.gz 2>/dev/null | awk '{print $5, $9}' | tr '\n' ' ')"
