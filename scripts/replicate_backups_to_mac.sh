#!/bin/bash
# Pull the pool boxes' daily backups onto the Mac mini (audit B22, operator decision #13).
# Run from the Mac (cron, e.g. 04:15 UTC, after both boxes' 03:30 backups):
#   replicate_backups_to_mac.sh [dest-dir]
# Needs SSH access from the Mac to both boxes with the same key the supervisor uses.
# Pulls only the backup directories (sqlite .backup snapshots, zecd keys.toml + config,
# pool config copies). The zecd age identity (spend authority) is never in a backup.
set -euo pipefail
DEST="${1:-$HOME/pool-backups}"
KEY="${POOL_SSH_KEY:-$HOME/.ssh/zebra_host}"
mkdir -p "$DEST/mainnet" "$DEST/testnet"
chmod 700 "$DEST" "$DEST/mainnet" "$DEST/testnet"
log() { echo "[$(date -u +%FT%TZ)] $*"; }
pull() {  # pull <label> <user@host> <remote-dir>
  local label="$1" host="$2" dir="$3"
  if rsync -az --timeout=120 -e "ssh -i $KEY -o ConnectTimeout=15 -o BatchMode=yes" \
      "$host:$dir/" "$DEST/$label/"; then
    log "$label: ok ($(ls "$DEST/$label" | wc -l) files, newest $(ls -t "$DEST/$label" | head -1))"
  else
    log "$label: FAILED"; return 1
  fi
}
status=0
pull mainnet zebra@38.190.136.77 /home/zebra/backups || status=1
pull testnet zec@74.80.181.116 /home/zec/backups || status=1
# Keep 30 days locally (the boxes keep 7).
find "$DEST" -name '*.db.gz' -mtime +30 -delete
exit $status
