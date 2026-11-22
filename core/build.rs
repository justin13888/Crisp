//! Embeds the Crisp ingredient index (`spec/ingredients/`) as one CBOR
//! dataset, so the native server and the WebAssembly client carry the
//! identical index with no loading path. The files are generated and
//! validated by `tools/ingredients`; `crisp_core::ingredients::index()`
//! validates the embedded copy again on first use.

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parsing {}: {e}", path.display()))
}

fn main() {
    let manifest =
        PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("cargo sets CARGO_MANIFEST_DIR"));
    let dir = manifest.join("../spec/ingredients");
    println!("cargo:rerun-if-changed={}", dir.display());

    let version = fs::read_to_string(dir.join("VERSION"))
        .unwrap_or_else(|e| panic!("reading {}/VERSION: {e}", dir.display()));
    let locales = read_json(&dir.join("locales.json"));
    let entries = read_json(&dir.join("entries.json"));
    let mut names: BTreeMap<String, Value> = BTreeMap::new();
    let mut files: Vec<PathBuf> = fs::read_dir(dir.join("names"))
        .unwrap_or_else(|e| panic!("listing {}/names: {e}", dir.display()))
        .map(|entry| entry.expect("directory entry").path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    files.sort();
    for path in files {
        let shard = path
            .file_stem()
            .and_then(|s| s.to_str())
            .expect("shard file names are UTF-8")
            .to_owned();
        names.insert(shard, read_json(&path));
    }

    let mut dataset = Map::new();
    dataset.insert(
        "version".to_owned(),
        Value::String(version.trim().to_owned()),
    );
    dataset.insert("locales".to_owned(), locales);
    dataset.insert("entries".to_owned(), entries);
    dataset.insert(
        "names".to_owned(),
        Value::Object(names.into_iter().collect()),
    );

    let mut bytes = Vec::new();
    ciborium::into_writer(&Value::Object(dataset), &mut bytes)
        .expect("encoding the dataset as CBOR");
    let out =
        PathBuf::from(env::var("OUT_DIR").expect("cargo sets OUT_DIR")).join("ingredients.cbor");
    fs::write(&out, bytes).unwrap_or_else(|e| panic!("writing {}: {e}", out.display()));
}
