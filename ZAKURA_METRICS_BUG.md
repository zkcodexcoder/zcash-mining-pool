# Unbounded per-connection label cardinality in network metrics makes `/metrics` unserveable

**Affected:** zakurad v1.1.0 (`1.1.0+g5ca43629306d`); still present in v1.1.1 (no metrics
changes in the diff) **and confirmed live in v1.2.0** (`1.2.0+g4f2189a`): after only
2.8 days of uptime, `/metrics` is already 91.5 MB / 1,121,361 lines with 353,571
`zcash_net_{in,out}_bytes_total` series — roughly double v1.1.0's daily series growth.
Mainnet, publicly reachable P2P (8233/tcp + 8234 v2).

## Symptom

After ~7 days of uptime, `GET /metrics` returns **117 MB / 1,430,869 lines** and takes
**~9.3 s** to serve. Any scraper with a conventional timeout fails (Prometheus default
10 s is marginal; our 5 s dashboard scraper failed mid-body with a chunked-read error).
Growth is unbounded: ~**34,000 new series (~17 MB) per day** on our node.

## Measurements (2026-08-14, uptime 6d 23h, 532 live peer connections)

Top families by line count:

```
 240,573  zcash_net_out_bytes_total
 239,959  zcash_net_in_bytes_total
 136,042  zcash_net_peers_connected
 134,916  zcash_net_peers_version_connected
 104,764  zcash_net_peers_obsolete
 104,734  zcash_net_peers_version_obsolete
 101,185  zcash_net_in_messages
  94,329  zcash_net_out_messages
  93,636  zakura_net_in_requests      (v2 stack affected too)
  83,085  zakura_net_out_responses
  44,052  zakura_net_out_requests
  30,060  zakura_net_in_responses
```

That is ~240k distinct label sets against **532 actual live connections** — a ~450:1
dead-to-live ratio.

## Root cause

Network metrics are labeled by remote socket address **including the ephemeral port**:

```
zcash_net_out_bytes_total{addr="18.211.216.54:57306"} 148
zcash_net_out_bytes_total{addr="95.216.77.237:40056"} 7607
```

Every reconnection (and every inbound scanner hitting the open P2P port) mints a new
`addr` label value, and series are never pruned on disconnect. The registry — and the
node's RSS along with it — grows for the life of the process (our node: 2.7 GB RSS at
7 days, a substantial share of which is the metrics registry).

## Impact

- `/metrics` becomes unusable with standard scrape timeouts within days.
- Serving it costs the node ~117 MB of I/O + serialization per scrape.
- Unbounded memory growth for long-lived nodes.
- An unauthenticated remote can accelerate the leak by cycling P2P connections
  (each SYN+handshake = another permanent series) — a low-rate memory-exhaustion vector
  for any node with a public P2P port.

## Suggested fixes (any one resolves it)

1. Drop the per-`addr` labels on counters and export aggregate totals (per-direction),
   keeping a bounded `peers` gauge for connection counts; or
2. Prune a connection's series on disconnect (remove label sets when the peer goes away); or
3. Label by a bounded key (e.g. IP without ephemeral port, capped set with an
   `addr="other"` overflow bucket).

## Repro

Run a mainnet node with a publicly reachable P2P port for several days, then:

```
curl -s http://localhost:9998/metrics | wc -cl
curl -s http://localhost:9998/metrics | sed -E 's/\{.*//; s/ .*//' | sort | uniq -c | sort -rn | head
```

Happy to provide the full metric dump or run instrumented builds.
