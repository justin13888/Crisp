//! Generates the typed Crisp API client from the server's OpenAPI contract,
//! exactly as `wasm/build.rs` does for the web client.

fn main() {
    let out_dir = std::env::var("OUT_DIR").expect("cargo sets OUT_DIR");
    println!("cargo:rerun-if-changed=../spec/openapi.json");
    spargen::generate(
        &spargen::Spec::new("../spec/openapi.json")
            .time(false)
            .uuid(false)
            .build(format!("{out_dir}/api.rs")),
    )
    .expect_success();
}
