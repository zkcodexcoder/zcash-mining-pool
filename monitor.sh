#!/bin/bash
# Background health monitor. Writes a snapshot every 5 minutes to monitor.log.
# Started before an 8h sleep; review when you wake up.
set -u
LOG=/home/zebra/zecminer/pool/monitor.log
POOL_LOG=/home/zebra/zecminer/pool/pool.log
DASH_LOG=/home/zebra/zecminer/pool/dashboard.log
ZALLET_LOG=/home/zebra/zallet.log

while true; do
    ts=$(date '+%Y-%m-%d %H:%M:%S')
    {
        echo "=== $ts ==="
        # Processes
        pool_pid=$(pgrep -f 'target/release/zcash-pool' | head -1)
        dash_pid=$(pgrep -f 'target/release/zcash-dashboard' | head -1)
        zallet_pid=$(pgrep -f 'target/release/zallet' | head -1)
        echo "  pool: ${pool_pid:-DOWN} | dashboard: ${dash_pid:-DOWN} | zallet: ${zallet_pid:-DOWN}"

        # Counts over last 5 min (300s)
        since=$(date -d '5 min ago' '+%Y-%m-%dT%H:%M')
        # Handle missing log files gracefully.
        pool_errors=$(grep -c " ERROR" "$POOL_LOG" 2>/dev/null || echo 0)
        dash_errors=$(grep -cE "ERROR|Payout round failed" "$DASH_LOG" 2>/dev/null || echo 0)
        zallet_chain_errs=$(tail -n 2000 "$ZALLET_LOG" 2>/dev/null | grep -c "Chain changed during block fetch" || echo 0)

        # Recent warnings
        recent_job_not_found=$(tail -n 1000 "$POOL_LOG" 2>/dev/null | grep -c "Job not found" || echo 0)
        recent_low_diff=$(tail -n 1000 "$POOL_LOG" 2>/dev/null | grep -c "Low difficulty share" || echo 0)
        recent_retargets=$(tail -n 1000 "$POOL_LOG" 2>/dev/null | grep -c "Vardiff retarget" || echo 0)
        new_blocks=$(tail -n 2000 "$POOL_LOG" 2>/dev/null | grep -c "New block detected" || echo 0)
        # Longpoll health
        longpoll_fails_total=$(grep -c "Longpoll failed" "$POOL_LOG" 2>/dev/null || echo 0)
        longpoll_fails_recent=$(tail -n 2000 "$POOL_LOG" 2>/dev/null | grep -c "Longpoll failed" || echo 0)

        echo "  errors total: pool=$pool_errors dashboard=$dash_errors"
        echo "  last 1000 lines: job_not_found=$recent_job_not_found low_diff=$recent_low_diff retargets=$recent_retargets"
        echo "  longpoll fails: total=$longpoll_fails_total last2000lines=$longpoll_fails_recent"
        echo "  zallet chain_changed (last 2000 lines): $zallet_chain_errs"

        # Pool DB status: last share counter values
        if [ -f /home/zebra/zecminer/pool/pool.db ]; then
            sqlite3 /home/zebra/zecminer/pool/pool.db "SELECT key, value FROM pool_status WHERE key IN ('shares_accepted','shares_rejected','rejects_low_diff','rejects_job_not_found','rejects_other','last_template_at_ms');" 2>/dev/null | sed 's/^/  /'
        fi

        # Wallet balance
        bal=$(curl -s -H 'Content-Type: application/json' --user pool:YImXRqpOmJvKQuOIKvRhRINt \
                -d '{"jsonrpc":"2.0","id":1,"method":"z_gettotalbalance","params":[1,true]}' \
                http://127.0.0.1:28232/ 2>/dev/null | python3 -c "import sys,json; d=json.load(sys.stdin).get('result',{}); print(f\"t={d.get('transparent','?')} p={d.get('private','?')}\")" 2>/dev/null)
        echo "  wallet: ${bal:-unreachable}"

        # Last new-block timestamp (from pool log)
        last_block=$(grep -oE "[0-9T:.-]+Z.*New block detected" "$POOL_LOG" 2>/dev/null | tail -1 | awk '{print $1}')
        echo "  last new-block detection: ${last_block:-none this session}"

        # Flag red-flag conditions
        flags=""
        [ -z "$pool_pid" ] && flags="$flags POOL_DOWN"
        [ -z "$dash_pid" ] && flags="$flags DASHBOARD_DOWN"
        [ -z "$zallet_pid" ] && flags="$flags ZALLET_DOWN"
        [ "$zallet_chain_errs" -gt 500 ] && flags="$flags ZALLET_STUCK"
        [ "$longpoll_fails_recent" -gt 50 ] && flags="$flags LONGPOLL_FAILING_OFTEN"
        [ -n "$flags" ] && echo "  *** FLAGS:$flags ***"

        echo ""
    } >> "$LOG"

    sleep 300
done
