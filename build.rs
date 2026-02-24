use std::path::Path;

fn main() {
    let files = [
        "schema/chain.capnp",
        "schema/common.capnp",
        "schema/echo.capnp",
        "schema/handler.capnp",
        "schema/init.capnp",
        "schema/mining.capnp",
        "schema/node.capnp",
        "schema/tracing.capnp",
        "schema/wallet.capnp",
        "schema/mp/proxy.capnp",
    ];

    for file in files {
        println!("cargo:rerun-if-changed={file}");
    }

    let mut cmd = capnpc::CompilerCommand::new();
    cmd.src_prefix("schema")
        .import_path("schema")
        .output_path(Path::new(&std::env::var("OUT_DIR").expect("OUT_DIR should be set")));

    for file in files {
        cmd.file(file);
    }

    cmd.run().expect("capnp codegen failed");
}
