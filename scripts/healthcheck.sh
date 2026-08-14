#!/usr/bin/env bash
# Pool healthcheck + Telegram alerting (audit #18 — no external alerting existed).
# Cron: */5 * * * *  — silent when healthy; alerts on state CHANGE (breach and
# recovery), re-alerts every 6h while still bad, daily 09:00 UTC heartbeat.
# Config: /home/zebra/.config/pool-alerts/telegram.env  (TG_TOKEN=..., TG_CHAT=...)
# If unconfigured, checks still run and log; sending is skipped.
set -u
CFG=/home/zebra/.config/pool-alerts
STATE=$CFG/state
mkdir -p "$STATE"
[ -f "$CFG/telegram.env" ] && . "$CFG/telegram.env"
POOL_DIR=/home/zebra/zecminer/pool
DB=$POOL_DIR/pool.db
NOW=$(date -u +%s)

send() { # send <text>
  [ -z "${TG_TOKEN:-}" ] || [ -z "${TG_CHAT:-}" ] && { echo "[$(date -u +%FT%TZ)] (no tg cfg) $1"; return; }
  curl -sS --max-time 15 "https://api.telegram.org/bot${TG_TOKEN}/sendMessage" \
    -d chat_id="${TG_CHAT}" -d disable_web_page_preview=true \
    --data-urlencode text="$1" >/dev/null 2>&1 || echo "[$(date -u +%FT%TZ)] tg send FAILED: $1"
}

# report <key> <ok:0|1> <bad-message> [good-message]
report() {
  local key=$1 ok=$2 bad=$3 good=${4:-"✅ recovered: $1"}
  local f="$STATE/$key"
  if [ "$ok" = "1" ]; then
    if [ -f "$f" ]; then rm -f "$f"; send "$good"; fi
  else
    local last=0; [ -f "$f" ] && last=$(cat "$f" 2>/dev/null || echo 0)
    if [ ! -f "$f" ] || [ $((NOW - last)) -ge 21600 ]; then
      echo "$NOW" > "$f"; send "🔴 $bad"
    fi
  fi
}

sq() { sqlite3 -readonly "$DB" "$1" 2>/dev/null; }

# --- checks (mainnet box) ---
P=$(systemctl is-active zcash-pool 2>/dev/null)
D=$(systemctl is-active zcash-dashboard 2>/dev/null)
Z=$(systemctl is-active zallet 2>/dev/null)
report svc_pool      "$([ "$P" = active ] && echo 1 || echo 0)" "zcash-pool is $P"
report svc_dashboard "$([ "$D" = active ] && echo 1 || echo 0)" "zcash-dashboard is $D"
report svc_zallet    "$([ "$Z" = active ] && echo 1 || echo 0)" "zallet is $Z"

MS=$(sq "SELECT value FROM pool_status WHERE key='last_template_at_ms';")
AGE=$(( (NOW*1000 - ${MS:-0}) / 1000 ))
report template "$([ -n "$MS" ] && [ $AGE -lt 180 ] && echo 1 || echo 0)" \
  "node templates stalled: last template ${AGE}s ago (pool mining stale work)"

SH=$(sq "SELECT COUNT(*) FROM shares WHERE created_at >= datetime('now','-10 minutes');")
report shares "$([ "${SH:-0}" -gt 0 ] && echo 1 || echo 0)" \
  "no shares in 10 min (miners disconnected or pool wedged)"

API=$(curl -s -o /dev/null -w "%{http_code}" --max-time 10 http://127.0.0.1:8080/api/stats 2>/dev/null)
report api "$([ "$API" = "200" ] && echo 1 || echo 0)" "dashboard API returned ${API:-timeout}"

NEG=$(sq "SELECT (SELECT COUNT(*) FROM balances WHERE pending<0)+(SELECT COUNT(*) FROM balances WHERE paying<0);")
report neg_balance "$([ "${NEG:-1}" = "0" ] && echo 1 || echo 0)" \
  "NEGATIVE BALANCE detected (neg rows: $NEG) — money-path invariant broken"

LOK=$(sq "SELECT (SELECT COALESCE(SUM(amount),0) FROM payouts)=(SELECT COALESCE(SUM(paid),0) FROM balances);")
report ledger "$([ "${LOK:-0}" = "1" ] && echo 1 || echo 0)" \
  "LEDGER DRIFT: sum(payouts) != sum(paid) — investigate before next payout"

LEAK=$(sq "SELECT COUNT(*) FROM payout_items pi JOIN payout_attempts pa ON pa.id=pi.attempt_id WHERE pa.status IN ('confirmed','failed');")
report reservation_leak "$([ "${LEAK:-1}" = "0" ] && echo 1 || echo 0)" \
  "payout reservation leak: $LEAK items on settled attempts"

DISK=$(df --output=pcent "$POOL_DIR" 2>/dev/null | tail -1 | tr -dc '0-9')
report disk "$([ "${DISK:-100}" -lt 90 ] && echo 1 || echo 0)" "disk at ${DISK}% on pool volume"

# --- reconciler alerts: forward NEW ones verbatim (dedupe by hash) ---
RH=$(sq "SELECT value FROM pool_status WHERE key='reconciler_health';")
if [ -n "$RH" ]; then
  echo "$RH" | python3 - "$STATE" <<'PY' 2>/dev/null | while IFS= read -r line; do send "⚠️ reconciler: $line"; done
import sys, json, hashlib, os
state = sys.argv[1]
try: d = json.load(sys.stdin)
except Exception: sys.exit(0)
seen_f = os.path.join(state, "reconciler_seen")
seen = set(open(seen_f).read().split("\n")) if os.path.exists(seen_f) else set()
out = []
for a in (d.get("alerts") or []):
    h = hashlib.sha1(a.encode()).hexdigest()[:16]
    if h not in seen:
        out.append(a); seen.add(h)
open(seen_f, "w").write("\n".join(list(seen)[-500:]))
for a in out[:5]: print(a[:400])
PY
fi

# --- testnet reachability (light) ---
TN_OK=$(ssh -i /home/zebra/.ssh/zebra_host -o BatchMode=yes -o ConnectTimeout=8 zec@operational-host.invalid \
  'systemctl is-active zcash-pool' 2>/dev/null)
report testnet "$([ "$TN_OK" = "active" ] && echo 1 || echo 0)" \
  "testnet pool unreachable or inactive ($TN_OK)"

# --- daily heartbeat 09:00-09:04 UTC ---
H=$(date -u +%H%M)
if [ "$H" -ge 0900 ] && [ "$H" -le 0904 ] && [ ! -f "$STATE/hb_$(date -u +%F)" ]; then
  rm -f "$STATE"/hb_* 2>/dev/null
  touch "$STATE/hb_$(date -u +%F)"
  BLOCKS=$(sq "SELECT COUNT(*) FROM blocks WHERE created_at >= datetime('now','-1 day');")
  PAID=$(sq "SELECT printf('%.4f', COALESCE(SUM(amount),0)/1e8) FROM payouts WHERE created_at >= datetime('now','-1 day');")
  PEND=$(sq "SELECT printf('%.4f', COALESCE(SUM(pending),0)/1e8) FROM balances;")
  send "💓 daily: all monitored checks green. 24h: blocks=$BLOCKS paid=${PAID} ZEC, pending=${PEND} ZEC, template ${AGE}s, shares10m=$SH"
fi
