fn main() {
    println!("cargo:rerun-if-changed=proto");
    std::env::set_current_dir("proto").expect("enter Protobuf module");
    connectrpc_build::Config::new()
        .use_buf()
        .files(&[
            "lgtm/v1/hello.proto",
            "lgtm/v1/review.proto",
            "lgtm/v1/diff.proto",
        ])
        .emit_rerun_directives(false)
        .include_file("_connectrpc.rs")
        .compile()
        .expect("compile LGTM Protobuf service");
}
