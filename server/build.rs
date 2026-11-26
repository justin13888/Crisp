//! Embeds the committed ingredient overrides
//! (`tools/ingredients/rules/overrides/names/*.json` and `merges.json`) so
//! the suggestion export (`ingredients::export`) can merge its lines into
//! the keys those files already hold rather than append duplicates. The
//! embedded index in `spec/ingredients/` is generated from these same files
//! (`just ingredients-check` keeps the two in step).

use std::env;
use std::fmt::Write as _;
use std::fs;
use std::path::PathBuf;

fn main() {
    let manifest =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let dir = manifest.join("../tools/ingredients/rules/overrides");
    println!("cargo:rerun-if-changed={}", dir.display());
    let names_dir = dir.join("names");
    println!("cargo:rerun-if-changed={}", names_dir.display());

    let mut shards: Vec<(String, PathBuf)> = fs::read_dir(&names_dir)
        .unwrap_or_else(|e| panic!("listing {}: {e}", names_dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .map(|p| {
            let shard = p
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("a UTF-8 shard file name")
                .to_owned();
            (shard, p)
        })
        .collect();
    shards.sort();

    let mut out = String::from("/// The committed `names/<shard>.json` overrides, by shard.\n");
    out.push_str("pub const NAMES: &[(&str, &str)] = &[\n");
    for (shard, path) in &shards {
        println!("cargo:rerun-if-changed={}", path.display());
        writeln!(
            out,
            "    ({shard:?}, include_str!({:?})),",
            path.display().to_string()
        )
        .expect("writing to a String");
    }
    out.push_str("];\n");
    let merges = dir.join("merges.json");
    println!("cargo:rerun-if-changed={}", merges.display());
    writeln!(
        out,
        "/// The committed `merges.json` override.\npub const MERGES: &str = include_str!({:?});",
        merges.display().to_string()
    )
    .expect("writing to a String");

    let target =
        PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("overrides.rs");
    fs::write(&target, out).unwrap_or_else(|e| panic!("writing {}: {e}", target.display()));
}
