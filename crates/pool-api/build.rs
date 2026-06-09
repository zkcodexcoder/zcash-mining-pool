// Generates Rust code from proto/lwd_service.proto for the lwd_tip module.
// Output lands in $OUT_DIR/cash.z.wallet.sdk.rpc.rs and is pulled in via
// `tonic::include_proto!("cash.z.wallet.sdk.rpc")` in lwd_tip.rs.

fn main() {
    tonic_build::configure()
        .build_server(false)
        .compile_protos(&["proto/lwd_service.proto"], &["proto"])
        .expect("failed to compile lwd_service.proto");
    println!("cargo:rerun-if-changed=proto/lwd_service.proto");
}
