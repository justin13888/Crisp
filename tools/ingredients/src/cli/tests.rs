//! End-to-end tests of the commands against a temporary repository tree.

use std::io::Write as _;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use super::*;

const TAXONOMY: &str = "\
en: vegetable

< en: vegetable
en: kimchi
ko: 김치
zh: 泡菜
";

const STCHARS: &str = "泡\t泡\n菜\t菜\n";

fn sha(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// A temporary tree with pinned sources, rules, and no published dataset.
struct Tree {
    paths: Paths,
}

impl Tree {
    fn new(name: &str) -> Self {
        let root =
            std::env::temp_dir().join(format!("crisp-ingredients-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let paths = Paths {
            rules: root.join("rules"),
            cache: root.join("cache"),
            root,
        };
        std::fs::create_dir_all(paths.rules.join("overrides/names")).expect("rules dir");
        std::fs::create_dir_all(&paths.cache).expect("cache dir");
        std::fs::write(paths.cache.join("off.txt"), TAXONOMY).expect("off");
        std::fs::write(paths.cache.join("st.txt"), STCHARS).expect("opencc");
        let zip_path = paths.cache.join("fdc.zip");
        {
            let mut zip = zip::ZipWriter::new(std::fs::File::create(&zip_path).expect("zip"));
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("sr/food.csv", options).expect("food");
            zip.write_all(b"fdc_id,description\n1,\"Kimchi\"\n")
                .expect("write");
            zip.start_file("sr/food_portion.csv", options)
                .expect("portion");
            zip.write_all(b"fdc_id,amount,modifier,gram_weight\n1,1,cup,150\n")
                .expect("write");
            zip.finish().expect("finish");
        }
        let fdc = std::fs::read(&zip_path).expect("zip bytes");
        let rules = format!(
            r#"
[sources.off]
url = "https://example.com/off.txt"
file = "off.txt"
sha256 = "{off}"
[sources.fdc]
url = "https://example.com/fdc.zip"
file = "fdc.zip"
sha256 = "{fdc}"
[sources.opencc.ST]
url = "https://example.com/st.txt"
file = "st.txt"
sha256 = "{st}"

[locales]
shards = ["en-US", "en-GB", "en-CA", "ko-KR", "zh-Hans-CN", "zh-Hant-TW"]
last_resort = ["en-US"]
english_lexicon = "en-regional.toml"
english = {{ us = "en-US", gb = "en-GB", ca = "en-CA" }}
[locales.off_language]
en = "en-US"
ko = "ko-KR"
zh = "zh-Hans-CN"
[locales.fallback]
"en" = ["en-US"]
[[locales.derive]]
shard = "zh-Hant-TW"
from = "zh-Hans-CN"
stages = [["ST"]]

[select]
include = [{{ id = "en:vegetable", category = "produce" }}]

[hints.allergens]
[hints.diet_when_no]
[hints.diet_from_allergen]

[density]
accept = ["off", "linked", "exact"]
denominator = 100
[density.volume_units]
cup = 250.0
"#,
            off = sha(TAXONOMY.as_bytes()),
            fdc = sha(&fdc),
            st = sha(STCHARS.as_bytes()),
        );
        std::fs::write(paths.rules.join("rules.toml"), rules).expect("rules");
        std::fs::write(
            paths.rules.join("en-regional.toml"),
            "[[term]]\nus = \"zucchini\"\ngb = \"courgette\"\nca = \"zucchini\"\n",
        )
        .expect("lexicon");
        std::fs::write(
            paths.rules.join("overrides/names/ko-KR.json"),
            "{\"kimchi\": {\"aliases\": [\"배추김치\"]}}\n",
        )
        .expect("override");
        Self { paths }
    }

    fn file(&self, relative: &str) -> PathBuf {
        self.paths.root.join(relative)
    }
}

impl Drop for Tree {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.paths.root).ok();
    }
}

fn run_ok(command: &str, tree: &Tree) -> String {
    let mut out = Vec::new();
    run(command, &tree.paths, &mut out).unwrap_or_else(|e| panic!("{command}: {e:?}"));
    String::from_utf8(out).expect("utf-8")
}

#[test]
fn sources_lists_every_pin() {
    let tree = Tree::new("sources");
    let out = run_ok("sources", &tree);
    assert_eq!(
        out.lines().collect::<Vec<_>>(),
        [
            "https://example.com/off.txt off.txt",
            "https://example.com/fdc.zip fdc.zip",
            "https://example.com/st.txt st.txt",
        ]
    );
}

#[test]
fn generate_writes_a_valid_dataset_that_check_then_accepts() {
    let tree = Tree::new("generate");
    assert!(
        run("check", &tree.paths, &mut Vec::new()).is_err(),
        "nothing generated yet"
    );
    run_ok("generate", &tree);
    let entries =
        std::fs::read_to_string(tree.file("spec/ingredients/entries.json")).expect("entries");
    assert!(entries.contains("\"slug\":\"kimchi\""), "{entries}");
    assert!(
        entries.contains("\"density\":[3,5]"),
        "density from the exact USDA match: {entries}"
    );
    let ko = std::fs::read_to_string(tree.file("spec/ingredients/names/ko-KR.json")).expect("ko");
    assert!(ko.contains("배추김치"), "{ko}");
    let tw =
        std::fs::read_to_string(tree.file("spec/ingredients/names/zh-Hant-TW.json")).expect("tw");
    assert!(tw.contains("泡菜"), "{tw}");
    assert_eq!(
        std::fs::read_to_string(tree.file("spec/ingredients/VERSION"))
            .expect("version")
            .trim()
            .len(),
        64
    );
    assert!(tree.file("tools/ingredients/report/report.md").exists());
    run_ok("check", &tree);

    std::fs::write(tree.file("spec/ingredients/names/en-US.json"), "{}\n").expect("tamper");
    let stale = run("check", &tree.paths, &mut Vec::new()).expect_err("stale");
    assert!(
        stale.to_string().contains("1 generated file(s) are stale"),
        "{stale}"
    );
}

#[test]
fn a_source_that_fails_its_pin_stops_the_import() {
    let tree = Tree::new("pin");
    std::fs::write(tree.paths.cache.join("off.txt"), "en: tampered\n").expect("tamper");
    let err = run("generate", &tree.paths, &mut Vec::new()).expect_err("pin mismatch");
    assert!(format!("{err:?}").contains("but the rules pin"), "{err:?}");
    std::fs::remove_file(tree.paths.cache.join("off.txt")).expect("remove");
    let err = run("generate", &tree.paths, &mut Vec::new()).expect_err("missing source");
    assert!(format!("{err:?}").contains("just ingredients"), "{err:?}");
}

#[test]
fn overrides_are_optional_but_must_parse() {
    let tree = Tree::new("overrides");
    std::fs::remove_file(tree.paths.rules.join("overrides/names/ko-KR.json")).expect("remove");
    run_ok("generate", &tree);
    std::fs::write(tree.paths.rules.join("overrides/merges.json"), "not json").expect("bad");
    assert!(run("generate", &tree.paths, &mut Vec::new()).is_err());
    std::fs::create_dir_all(tree.paths.rules.join("overrides/entries.json"))
        .expect("dir in the way");
    assert!(
        read_json_or::<Vec<String>>(&tree.paths.rules.join("overrides/entries.json"), Vec::new())
            .is_err()
    );
}

#[test]
fn an_unknown_dictionary_or_command_is_an_error() {
    let tree = Tree::new("unknown");
    let rules = std::fs::read_to_string(tree.paths.rules.join("rules.toml")).expect("rules");
    std::fs::write(
        tree.paths.rules.join("rules.toml"),
        rules.replace("[[\"ST\"]]", "[[\"NOPE\"]]"),
    )
    .expect("rewrite");
    let err = run("generate", &tree.paths, &mut Vec::new()).expect_err("unknown dictionary");
    assert!(
        format!("{err:?}").contains("unknown OpenCC dictionary NOPE"),
        "{err:?}"
    );
    assert!(run("frobnicate", &tree.paths, &mut Vec::new()).is_err());
}

#[test]
fn the_repository_paths_point_at_the_committed_rules() {
    let paths = Paths::repository();
    assert!(paths.rules.join("rules.toml").is_file());
    assert!(paths.root.join("spec").is_dir());
    assert!(paths.cache.ends_with(Path::new("target/ingredients-cache")));
}
