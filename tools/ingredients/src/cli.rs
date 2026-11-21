//! The importer's commands over explicit paths, so the whole pipeline runs
//! in tests against a temporary tree as it runs against the repository.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crisp_core::ingredients::{Entry, IngredientIndex};
use eyre::{Context, Result, bail, eyre};
use sha2::{Digest, Sha256};
use tracing::{info, warn};

use crate::assemble::{Inputs, assemble};
use crate::english::Lexicon;
use crate::names::NameOverride;
use crate::opencc::{Converter, Stage};
use crate::output::{DATASET_DIR, Generated, dataset_files, review_files};
use crate::rules::{Rules, Source};
use crate::{names, off, rules, usda};

/// Where the importer reads and writes.
#[derive(Debug, Clone)]
pub struct Paths {
    /// The repository root; generated paths are relative to it.
    pub root: PathBuf,
    /// The rules directory (`tools/ingredients/rules`).
    pub rules: PathBuf,
    /// The source cache (`target/ingredients-cache`).
    pub cache: PathBuf,
}

impl Paths {
    /// The paths of this repository checkout.
    #[must_use]
    pub fn repository() -> Self {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        Self {
            rules: root.join("tools/ingredients/rules"),
            cache: root.join("target/ingredients-cache"),
            root,
        }
    }
}

/// Reads a pinned source, failing unless its hash matches the pin.
fn read_pinned(paths: &Paths, source: &Source) -> Result<Vec<u8>> {
    let path = paths.cache.join(&source.file);
    let bytes = std::fs::read(&path).wrap_err_with(|| {
        format!(
            "reading {} (run `just ingredients` to fetch it)",
            path.display()
        )
    })?;
    let digest: String = Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    if digest != source.sha256 {
        bail!(
            "{} has SHA-256 {digest}, but the rules pin {}",
            source.file,
            source.sha256
        );
    }
    Ok(bytes)
}

/// Reads JSON, or `default` when the file does not exist.
fn read_json_or<T: serde::de::DeserializeOwned>(path: &Path, default: T) -> Result<T> {
    match std::fs::read_to_string(path) {
        Ok(text) => {
            serde_json::from_str(&text).wrap_err_with(|| format!("parsing {}", path.display()))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(default),
        Err(e) => Err(e).wrap_err_with(|| format!("reading {}", path.display())),
    }
}

/// Builds one OpenCC converter per derived shard.
fn converters(paths: &Paths, rules: &Rules) -> Result<BTreeMap<String, Converter>> {
    let mut texts: BTreeMap<&str, String> = BTreeMap::new();
    for (name, source) in &rules.sources.opencc {
        let bytes = read_pinned(paths, source)?;
        texts.insert(
            name,
            String::from_utf8(bytes).wrap_err_with(|| format!("OpenCC {name}"))?,
        );
    }
    let mut out = BTreeMap::new();
    for d in &rules.locales.derive {
        let mut stages = Vec::new();
        for stage in &d.stages {
            let dicts: Vec<&str> = stage
                .iter()
                .map(|n| {
                    texts
                        .get(n.as_str())
                        .map(String::as_str)
                        .ok_or_else(|| eyre!("unknown OpenCC dictionary {n}"))
                })
                .collect::<Result<_>>()?;
            stages.push(Stage::from_texts(&dicts));
        }
        out.insert(d.shard.clone(), Converter::new(stages));
    }
    Ok(out)
}

/// Runs the pipeline and renders every generated file.
fn generate(paths: &Paths) -> Result<Generated> {
    let rules = rules::load(&paths.rules.join("rules.toml"))?;
    let off_text =
        String::from_utf8(read_pinned(paths, &rules.sources.off)?).wrap_err("OFF taxonomy")?;
    read_pinned(paths, &rules.sources.fdc)?;
    let taxonomy = off::parse(&off_text);
    info!(
        entries = taxonomy.entries.len(),
        unresolved = taxonomy.unresolved.len(),
        "OFF taxonomy parsed"
    );
    let foods = usda::load(
        &paths.cache.join(&rules.sources.fdc.file),
        &rules.density.volume_units,
    )?;
    info!(foods = foods.len(), "USDA densities derived");
    let lexicon = Lexicon::load(&paths.rules.join(&rules.locales.english_lexicon))?;
    let converters = converters(paths, &rules)?;
    let added: Vec<Entry> = read_json_or(&paths.rules.join("overrides/entries.json"), Vec::new())?;
    let mut name_overrides: BTreeMap<String, BTreeMap<String, NameOverride>> = BTreeMap::new();
    for shard in &rules.locales.shards {
        let path = paths.rules.join(format!("overrides/names/{shard}.json"));
        let map: BTreeMap<String, NameOverride> = read_json_or(&path, BTreeMap::new())?;
        name_overrides.insert(shard.clone(), map);
    }
    let merges: BTreeMap<String, String> =
        read_json_or(&paths.rules.join("overrides/merges.json"), BTreeMap::new())?;
    let mut previous_names = names::ShardNames::new();
    for shard in &rules.locales.shards {
        let path = paths
            .root
            .join(DATASET_DIR)
            .join(format!("names/{shard}.json"));
        let map: BTreeMap<String, crisp_core::ingredients::Names> =
            read_json_or(&path, BTreeMap::new())?;
        previous_names.insert(shard.clone(), map);
    }
    let previous: Vec<Entry> = read_json_or(
        &paths.root.join(DATASET_DIR).join("entries.json"),
        Vec::new(),
    )?;
    let assembled = assemble(
        &rules,
        &Inputs {
            taxonomy: &taxonomy,
            foods: &foods,
            lexicon: &lexicon,
            converters: &converters,
            added: &added,
            name_overrides: &name_overrides,
            previous: &previous,
            previous_names: &previous_names,
            merges: &merges,
        },
    )?;
    let mut files = dataset_files(&assembled.dataset)?;
    let version = files
        .last()
        .map(|(_, v)| v.trim().to_owned())
        .unwrap_or_default();
    let mut dataset = assembled.dataset.clone();
    dataset.version.clone_from(&version);
    IngredientIndex::new(dataset).wrap_err("the generated dataset is not a valid index")?;
    files.extend(review_files(&assembled, &version)?);
    info!(entries = assembled.dataset.entries.len(), %version, "ingredient index generated");
    Ok(files)
}

/// Runs one command (`sources`, `generate`, or `check`), writing `sources`
/// output to `out`.
///
/// # Errors
/// An unknown command; a source missing or failing its pin; invalid rules
/// or overrides; an invalid generated index; or, for `check`, any stale file.
pub fn run(command: &str, paths: &Paths, out: &mut impl std::io::Write) -> Result<()> {
    match command {
        "sources" => {
            let rules = rules::load(&paths.rules.join("rules.toml"))?;
            for source in rules.sources.all() {
                writeln!(out, "{} {}", source.url, source.file)?;
            }
        }
        "generate" => {
            for (path, content) in generate(paths)? {
                let path = paths.root.join(path);
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(&path, content)
                    .wrap_err_with(|| format!("writing {}", path.display()))?;
            }
        }
        "check" => {
            let mut stale = Vec::new();
            for (path, content) in generate(paths)? {
                let current = std::fs::read_to_string(paths.root.join(&path)).unwrap_or_default();
                if current != content {
                    warn!(%path, "stale generated file");
                    stale.push(path);
                }
            }
            if !stale.is_empty() {
                bail!(
                    "{} generated file(s) are stale; run `just ingredients`",
                    stale.len()
                );
            }
        }
        other => bail!("unknown command {other:?}; expected sources, generate, or check"),
    }
    Ok(())
}

#[cfg(test)]
mod tests;
