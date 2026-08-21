#!/usr/bin/env bash
# Pool healthcheck + Telegram alerting (audit #18 — no external alerting existed).
# Cron: */5 * * * *  — silent when healthy; alerts on state CHANGE (breach and
# recovery), re-alerts every 6h while still bad, daily 09:00 UTC heartbeat.
# Config: /home/zebra/.config/pool-alerts/telegram.env  (TG_TOKEN=..., TG_CHAT=...)
# Hosts:  /home/zebra/.config/pool-alerts/hosts.env
# If unconfigured, checks still run and log; sending is skipped.
set -u
CFG=/home/zebra/.config/pool-alerts
STATE=$CFG/state
mkdir -p "$STATE"
[ -f "$CFG/telegram.env" ] && . "$CFG/telegram.env"
[ -f "$CFG/hosts.env" ] && . "$CFG/hosts.env"
MAINNET_NODE_HOST=${MAINNET_NODE_HOST:-zakura-mainnet.internal}
TESTNET_POOL_HOST=${TESTNET_POOL_HOST:-pool.tazminer.com}
TESTNET_NODE_HOST=${TESTNET_NODE_HOST:-zakura-testnet.internal}
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

LOAD5=$(cut -d' ' -f2 /proc/loadavg)
report load_pool_box "$(awk -v l="$LOAD5" 'BEGIN{print (l<6.0)?1:0}')" \
  "pool box load5 is $LOAD5 (sustained high CPU)"

# --- zallet failure modes (2026-08-19: testnet crash-looped 24x before a
# payout finally surfaced it; `is-active` misses loops because systemd
# revives the service in seconds) ---
# (a) crash-loop: NRestarts delta >= 3 within an hour
NR=$(systemctl show zallet -p NRestarts --value 2>/dev/null)
if [ -n "$NR" ]; then
  if [ -f "$STATE/zallet_nr_window" ]; then read -r NR0 NRT < "$STATE/zallet_nr_window"; else NR0=$NR; NRT=$NOW; fi
  if [ $((NOW - NRT)) -ge 3600 ]; then echo "$NR $NOW" > "$STATE/zallet_nr_window"; NR0=$NR; fi
  [ ! -f "$STATE/zallet_nr_window" ] && echo "$NR $NOW" > "$STATE/zallet_nr_window"
  report zallet_crashloop "$([ $((NR - NR0)) -lt 3 ] && echo 1 || echo 0)" \
    "zallet CRASH-LOOP: $((NR - NR0)) restarts within the hour (service still reads active between deaths)"
fi
# (a2) admin/ops brute-force attempts (audit #18): the dashboard logs every
# failed login and every rate-limit rejection; alert when they appear.
# Threshold 3: a couple of operator typos shouldn't page; an attack is dozens.
AUTH=$(tail -c 200000 "$POOL_DIR/dashboard.log" 2>/dev/null | grep -cE "login failed \(bad password\)|login rate-limited")
report auth_bruteforce "$([ "${AUTH:-0}" -le 3 ] && echo 1 || echo 0)" \
  "admin/ops login attack: $AUTH failed/limited attempts in recent log — someone is guessing passwords"

# (b) note-commitment-tree corruption signature in recent log
TREE=$(tail -c 300000 /home/zebra/zallet.log 2>/dev/null | grep -cE "note commitment tree|Inserted root conflicts")
report zallet_tree_corruption "$([ "${TREE:-0}" = "0" ] && echo 1 || echo 0)" \
  "zallet NOTE-TREE CORRUPTION in log ($TREE hits) — payouts/shielding will fail 'have 0'; fix: stop zallet, 'zallet repair truncate-wallet <min-allowed-height>', start"
# (c) wallet spendability/health as the dashboard sees it
PH=$(sq "SELECT value FROM pool_status WHERE key='payout_health';")
if [ -n "$PH" ]; then
  PHBAD=$(echo "$PH" | python3 -c '
import sys, json
try: d = json.load(sys.stdin)
except Exception: sys.exit(0)
resp = d.get("wallet_responsive", True)
fails = max(d.get("consecutive_payout_failures", 0), d.get("consecutive_shielding_failures", 0))
print("bad" if (not resp or fails >= 3) else "ok")' 2>/dev/null)
  report zallet_payout_health "$([ "$PHBAD" != "bad" ] && echo 1 || echo 0)" \
    "payout pipeline unhealthy: wallet unresponsive or >=3 consecutive payout/shielding failures (see admin health page)"
fi

# --- mainnet node (RPC direct + one ssh probe) ---
# EOS fuse of the RUNNING zakurad binary: v1.2.0 (cutover 2026-08-16) panics at
# ~3,495,707 (~Sept 25). Re-bump at every rebuild/cutover (task #17 treadmill).
EOS_PANIC_HEIGHT=3495707
H76=$(curl -s --max-time 8 --data-binary '{"jsonrpc":"1.0","id":"hc","method":"getblockcount","params":[]}' \
  -H 'content-type:text/plain;' "http://${MAINNET_NODE_HOST}:8232/" 2>/dev/null | python3 -c 'import sys,json;print(json.load(sys.stdin)["result"])' 2>/dev/null)
report node76_rpc "$([ -n "$H76" ] && echo 1 || echo 0)" "mainnet node RPC not answering (pool cannot get work)"
EOS_DAYS=""
if [ -n "$H76" ]; then
  # height-stall: alert if tip unchanged for 30 min (~24 expected blocks)
  if [ -f "$STATE/node76_height" ]; then read -r LH LT < "$STATE/node76_height"; else LH=0; LT=$NOW; fi
  if [ "$H76" != "$LH" ]; then echo "$H76 $NOW" > "$STATE/node76_height"; LT=$NOW; fi
  report node76_stall "$([ $((NOW - LT)) -lt 1800 ] && echo 1 || echo 0)" \
    "mainnet node tip stuck at $H76 for $(( (NOW-LT)/60 )) min (sync stalled?)"
  EOS_DAYS=$(( (EOS_PANIC_HEIGHT - H76) / 1152 ))
  report node76_eos "$([ "$EOS_DAYS" -ge 10 ] && echo 1 || echo 0)" \
    "zakurad EOS panic in ~${EOS_DAYS} days (height $H76 / $EOS_PANIC_HEIGHT) — cutover NOW (task #17)"
fi
N76=$(ssh -i /home/zebra/.ssh/zebra_host -o BatchMode=yes -o ConnectTimeout=8 "zebra@${MAINNET_NODE_HOST}" \
  'pgrep -x zakurad >/dev/null && echo up || echo down; df --output=pcent / | tail -1 | tr -dc 0-9; echo; cut -d" " -f2 /proc/loadavg' 2>/dev/null)
if [ -n "$N76" ]; then
  report node76_ssh 1 "" "✅ recovered: node76_ssh"
  report node76_zakurad "$([ "$(echo "$N76" | sed -n 1p)" = up ] && echo 1 || echo 0)" "zakurad process DOWN on mainnet node"
  report node76_disk "$([ "$(echo "$N76" | sed -n 2p)" -lt 90 ] 2>/dev/null && echo 1 || echo 0)" "disk at $(echo "$N76" | sed -n 2p)% on mainnet node"
  report node76_load "$(awk -v l="$(echo "$N76" | sed -n 3p)" 'BEGIN{print (l<6.0)?1:0}')" "mainnet node load5 is $(echo "$N76" | sed -n 3p)"
else
  report node76_ssh 0 "mainnet node unreachable over SSH"
fi

# --- testnet pool box (one ssh probe: services + disk + zallet failure modes) ---
TN=$(ssh -i /home/zebra/.ssh/zebra_host -o BatchMode=yes -o ConnectTimeout=8 "zec@${TESTNET_POOL_HOST}" \
  'systemctl is-active zcash-pool zcash-dashboard zallet | tr "\n" " "; echo
   df --output=pcent / | tail -1 | tr -dc 0-9; echo
   systemctl show zallet -p NRestarts --value
   tail -c 300000 ~/zallet.log 2>/dev/null | grep -cE "note commitment tree|Inserted root conflicts"' 2>/dev/null)
if [ -n "$TN" ]; then
  report testnet_ssh 1 "" "✅ recovered: testnet_ssh"
  TSVC=$(echo "$TN" | sed -n 1p)
  report testnet "$([ "$TSVC" = "active active active " ] && echo 1 || echo 0)" \
    "[testnet] services not all active (pool/dashboard/zallet = $TSVC)"
  report testnet_disk "$([ "$(echo "$TN" | sed -n 2p)" -lt 90 ] 2>/dev/null && echo 1 || echo 0)" "[testnet] disk at $(echo "$TN" | sed -n 2p)%"
  TNNR=$(echo "$TN" | sed -n 3p)
  if [ -n "$TNNR" ]; then
    if [ -f "$STATE/tn_zallet_nr_window" ]; then read -r TNNR0 TNNRT < "$STATE/tn_zallet_nr_window"; else TNNR0=$TNNR; TNNRT=$NOW; fi
    if [ $((NOW - TNNRT)) -ge 3600 ]; then echo "$TNNR $NOW" > "$STATE/tn_zallet_nr_window"; TNNR0=$TNNR; fi
    [ ! -f "$STATE/tn_zallet_nr_window" ] && echo "$TNNR $NOW" > "$STATE/tn_zallet_nr_window"
    report tn_zallet_crashloop "$([ $((TNNR - TNNR0)) -lt 3 ] && echo 1 || echo 0)" \
      "[testnet] zallet CRASH-LOOP: $((TNNR - TNNR0)) restarts within the hour"
  fi
  TNTREE=$(echo "$TN" | sed -n 4p)
  report tn_zallet_tree_corruption "$([ "${TNTREE:-0}" = "0" ] && echo 1 || echo 0)" \
    "[testnet] zallet NOTE-TREE CORRUPTION in log ($TNTREE hits) — fix: 'zallet repair truncate-wallet <min-allowed-height>'"
else
  report testnet_ssh 0 "[testnet] pool box unreachable over SSH"
fi

# --- testnet node (one ssh probe: process + local RPC height + disk) ---
N75=$(ssh -i /home/zebra/.ssh/zebra_host -o BatchMode=yes -o ConnectTimeout=8 "zebra@${TESTNET_NODE_HOST}" \
  'pgrep -x zakurad >/dev/null && echo up || echo down
   curl -s --max-time 5 --data-binary "{\"jsonrpc\":\"1.0\",\"id\":\"hc\",\"method\":\"getblockcount\",\"params\":[]}" -H "content-type:text/plain;" http://127.0.0.1:18232/ | python3 -c "import sys,json;print(json.load(sys.stdin)[\"result\"])" 2>/dev/null
   df --output=pcent / | tail -1 | tr -dc 0-9' 2>/dev/null)
if [ -n "$N75" ]; then
  report node75_ssh 1 "" "✅ recovered: node75_ssh"
  report node75_zakurad "$([ "$(echo "$N75" | sed -n 1p)" = up ] && echo 1 || echo 0)" "[testnet] zakurad process DOWN on testnet node"
  H75=$(echo "$N75" | sed -n 2p)
  report node75_rpc "$([ -n "$H75" ] && echo 1 || echo 0)" "[testnet] node RPC not answering"
  if [ -n "$H75" ]; then
    if [ -f "$STATE/node75_height" ]; then read -r LH75 LT75 < "$STATE/node75_height"; else LH75=0; LT75=$NOW; fi
    if [ "$H75" != "$LH75" ]; then echo "$H75 $NOW" > "$STATE/node75_height"; LT75=$NOW; fi
    report node75_stall "$([ $((NOW - LT75)) -lt 1800 ] && echo 1 || echo 0)" \
      "[testnet] node tip stuck at $H75 for $(( (NOW-LT75)/60 )) min"
  fi
  report node75_disk "$([ "$(echo "$N75" | sed -n 3p)" -lt 90 ] 2>/dev/null && echo 1 || echo 0)" "[testnet] disk at $(echo "$N75" | sed -n 3p)% on testnet node"
else
  report node75_ssh 0 "[testnet] node unreachable over SSH"
fi

# --- daily heartbeat 09:00-09:04 UTC ---
H=$(date -u +%H%M)
if [ "$H" -ge 0900 ] && [ "$H" -le 0904 ] && [ ! -f "$STATE/hb_$(date -u +%F)" ]; then
  rm -f "$STATE"/hb_* 2>/dev/null
  touch "$STATE/hb_$(date -u +%F)"
  BLOCKS=$(sq "SELECT COUNT(*) FROM blocks WHERE created_at >= datetime('now','-1 day');")
  PAID=$(sq "SELECT printf('%.4f', COALESCE(SUM(amount),0)/1e8) FROM payouts WHERE created_at >= datetime('now','-1 day');")
  PEND=$(sq "SELECT printf('%.4f', COALESCE(SUM(pending),0)/1e8) FROM balances;")
  send "💓 daily: all monitored checks green. 24h: blocks=$BLOCKS paid=${PAID} ZEC, pending=${PEND} ZEC, template ${AGE}s, shares10m=$SH. mainnet h=${H76:-?} (EOS in ~${EOS_DAYS:-?}d), testnet h=${H75:-?}"
fi
