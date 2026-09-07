//! Offline synthetic fixture generator, not a wallet or consensus-valid tx.
//! Compile with the workspace's already-built zcash_primitives/zcash_address/
//! zcash_protocol rlibs. Dummy proofs and signatures are deliberately invalid.
use zcash_address::{ToAddress, ZcashAddress};
use zcash_primitives::transaction::Transaction;
use zcash_protocol::consensus::NetworkType;

fn main() {
    let fee = std::env::args()
        .nth(1)
        .map(|s| s.parse::<i64>().unwrap())
        .unwrap_or(10_000);
    let point = [
        0xa9, 0xcb, 0x0d, 0x13, 0x72, 0x32, 0xff, 0x84, 0x48, 0xd0, 0xf0, 0x78, 0xb6, 0x81, 0x4c,
        0x66, 0xcb, 0x33, 0x1b, 0x0f, 0x2d, 0x3d, 0x8a, 0x08, 0x5b, 0xed, 0xba, 0x81, 0x5f, 0x00,
        0xa8, 0xdb,
    ];
    let mut raw = Vec::new();
    for value in [0x80000005u32, 0x26a7270a, 0xc2d6d0b4, 0, 2_000_040] {
        raw.extend(value.to_le_bytes());
    }
    raw.extend([0, 1]); // no transparent input, one P2PKH output
    raw.extend(1_000_000u64.to_le_bytes());
    raw.extend([25, 0x76, 0xa9, 0x14]);
    raw.extend([2; 20]);
    raw.extend([0x88, 0xac]);
    raw.push(1);
    raw.extend(point);
    raw.extend([0; 32]);
    raw.extend(point);
    raw.push(0);
    raw.extend((1_000_000i64 + fee).to_le_bytes());
    raw.extend([0; 32]);
    raw.extend([0; 192]);
    raw.extend([0; 64]);
    raw.extend([0; 64]);
    raw.push(0);
    let tx = Transaction::read(
        std::io::Cursor::new(&raw),
        0xc2d6d0b4u32.try_into().unwrap(),
    )
    .unwrap();
    let mut encoded = Vec::new();
    tx.write(&mut encoded).unwrap();
    assert_eq!(raw, encoded);
    println!(
        "source={}",
        ZcashAddress::from_sapling(NetworkType::Test, [1; 43]).encode()
    );
    println!(
        "recipient={}",
        ZcashAddress::from_transparent_p2pkh(NetworkType::Test, [2; 20]).encode()
    );
    println!("txid={}", tx.txid());
    println!(
        "raw={}",
        raw.iter().map(|b| format!("{b:02x}")).collect::<String>()
    );
}
