// Manual probe of the 9 lwd servers — used to verify the gRPC client
// works against real endpoints. Run with:
//   cargo run --release -p pool-api --example lwd_probe

use std::time::Instant;

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let servers: Vec<String> = vec![
        "eu.zec.rocks".into(),
        "eu.zec.stardust.rest".into(),
        "jp.zec.stardust.rest".into(),
        "us.zec.stardust.rest".into(),
        "eu2.zec.stardust.rest".into(),
        "zec.rocks".into(),
        "na.zec.rocks".into(),
        "sa.zec.rocks".into(),
        "ap.zec.rocks".into(),
    ];

    let start = Instant::now();
    let snap = pool_api::fetch_authoritative_tip(&servers).await;
    let elapsed = start.elapsed();

    println!("Aggregate:");
    println!("  max_height     = {:?}", snap.max_height);
    println!("  responses_ok   = {} / {}", snap.responses_ok, snap.responses_total);
    println!("  elapsed total  = {:.2}s", elapsed.as_secs_f32());
    println!();
    println!("Per-server heights (ok):");
    let mut ok: Vec<_> = snap.per_server_height.iter().collect();
    ok.sort_by_key(|(_, h)| std::cmp::Reverse(**h));
    for (host, h) in ok {
        println!("  {h}  {host}");
    }
    if !snap.per_server_error.is_empty() {
        println!();
        println!("Per-server errors:");
        for (host, e) in snap.per_server_error {
            println!("  {host:28}  {e}");
        }
    }
}
