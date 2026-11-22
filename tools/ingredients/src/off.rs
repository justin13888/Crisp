//! Parsing the Open Food Facts ingredients taxonomy (`ingredients.txt`).
//!
//! The file is a list of blank-line-separated blocks. In a block:
//!
//! - `< lang: name` names a parent by any of its names in that language;
//! - `lang: name, synonym, …` lists an entry's names in one language — the
//!   first language line's first name is its canonical id (`en:fish-sauce`);
//! - `property:lang: value` sets a property (`vegan:en: no`,
//!   `allergens:en: en:fish`, `usda_fdc_code:en: 171426`);
//! - `#` lines are comments; `stopwords:` and `synonyms:` blocks are skipped.

use std::collections::BTreeMap;

/// One taxonomy entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OffEntry {
    /// The canonical id, e.g. `en:fish-sauce`.
    pub id: String,
    /// Resolved parent ids, in the order the block lists them.
    pub parents: Vec<String>,
    /// Names per OFF language code, display name first.
    pub names: BTreeMap<String, Vec<String>>,
    /// Properties: name → (language → value).
    pub props: BTreeMap<String, String>,
}

/// The parsed taxonomy, plus what could not be resolved.
#[derive(Debug, Clone, Default)]
pub struct Taxonomy {
    /// Entries in file order.
    pub entries: Vec<OffEntry>,
    /// Parent references that name no entry, as `child → parent text`.
    pub unresolved: Vec<(String, String)>,
}

/// The OFF canonical form of a name: lowercase, whitespace and underscores
/// as hyphens.
#[must_use]
pub fn canonical(lang: &str, name: &str) -> String {
    let lowered = name.trim().to_lowercase();
    let joined: Vec<&str> = lowered
        .split(|c: char| c.is_whitespace() || c == '_')
        .filter(|s| !s.is_empty())
        .collect();
    format!("{lang}:{}", joined.join("-"))
}

/// Splits `lang: a, b` (or `prop:lang: v`) into its key and value.
fn key_value(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once(": ").or_else(|| {
        line.strip_suffix(':')
            .map(|k| (k, ""))
            .or_else(|| line.split_once(':'))
    })?;
    Some((key.trim(), value.trim()))
}

/// Whether `key` is a bare language code (`en`, `zh`, `zh_yue`).
fn is_language(key: &str) -> bool {
    let mut parts = key.split('_');
    let lang = parts.next().unwrap_or_default();
    (2..=3).contains(&lang.len())
        && lang.bytes().all(|b| b.is_ascii_lowercase())
        && parts.all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase()))
}

/// Parses the taxonomy text.
#[must_use]
pub fn parse(text: &str) -> Taxonomy {
    struct Raw {
        id: String,
        parents: Vec<(String, String)>,
        names: BTreeMap<String, Vec<String>>,
        props: BTreeMap<String, String>,
    }
    let mut raws: Vec<Raw> = Vec::new();
    // Trailing whitespace dropped, so a line of spaces still separates blocks.
    let normalized: String = text
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n");
    for block in normalized.split("\n\n") {
        // Blank lines parse as nothing, and `stopwords:`/`synonyms:` blocks
        // hold only properties, so neither yields an entry.
        let lines: Vec<&str> = block
            .lines()
            .map(str::trim)
            .filter(|l| !l.starts_with('#'))
            .collect();
        let mut parents = Vec::new();
        let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut first_lang: Option<String> = None;
        let mut props = BTreeMap::new();
        for line in lines {
            if let Some(rest) = line.strip_prefix('<') {
                if let Some((lang, name)) = key_value(rest.trim()) {
                    parents.push((lang.to_owned(), name.to_owned()));
                }
                continue;
            }
            let Some((key, value)) = key_value(line) else {
                continue;
            };
            if is_language(key) {
                let list: Vec<String> = value
                    .split(',')
                    .map(str::trim)
                    .filter(|n| !n.is_empty())
                    .map(str::to_owned)
                    .collect();
                if list.is_empty() {
                    continue;
                }
                if first_lang.is_none() {
                    first_lang = Some(key.to_owned());
                }
                names.entry(key.to_owned()).or_default().extend(list);
            } else if let Some((prop, _lang)) = key.rsplit_once(':') {
                props
                    .entry(prop.to_owned())
                    .or_insert_with(|| value.to_owned());
            }
        }
        let Some(lang) = first_lang else {
            continue;
        };
        let id = canonical(&lang, &names[&lang][0]);
        raws.push(Raw {
            id,
            parents,
            names,
            props,
        });
    }
    let mut by_name: BTreeMap<String, String> = BTreeMap::new();
    for raw in &raws {
        by_name
            .entry(raw.id.clone())
            .or_insert_with(|| raw.id.clone());
    }
    for raw in &raws {
        for (lang, list) in &raw.names {
            for name in list {
                by_name
                    .entry(canonical(lang, name))
                    .or_insert_with(|| raw.id.clone());
            }
        }
    }
    let mut taxonomy = Taxonomy::default();
    for raw in raws {
        let mut parents = Vec::new();
        for (lang, name) in &raw.parents {
            match by_name.get(&canonical(lang, name)) {
                Some(id) if *id != raw.id && !parents.contains(id) => parents.push(id.clone()),
                Some(_) => {}
                None => taxonomy
                    .unresolved
                    .push((raw.id.clone(), format!("{lang}: {name}"))),
            }
        }
        taxonomy.entries.push(OffEntry {
            id: raw.id,
            parents,
            names: raw.names,
            props: raw.props,
        });
    }
    taxonomy
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "\
# comment block

stopwords:en: the, of

synonyms:en: colour, color

en: vegetable, vegetables
fr: légume
vegan:en: yes

< en: vegetables
en: White cabbage
ko: 양배추

< en: white cabbage
< en: no such parent
en: kimchi
xx: kimchi
ko: 김치
allergens:en: en:fish
usda_fdc_code:en: 171426

fr: jus de chou
< fr: légume
";

    #[test]
    fn blocks_become_entries_with_ids_names_and_properties() {
        let t = parse(SAMPLE);
        let ids: Vec<&str> = t.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "en:vegetable",
                "en:white-cabbage",
                "en:kimchi",
                "fr:jus-de-chou"
            ]
        );
        let kimchi = &t.entries[2];
        assert_eq!(kimchi.names["ko"], ["김치"]);
        assert_eq!(kimchi.props["allergens"], "en:fish");
        assert_eq!(kimchi.props["usda_fdc_code"], "171426");
        assert_eq!(t.entries[0].props["vegan"], "yes");
    }

    #[test]
    fn parents_resolve_through_any_synonym_and_report_misses() {
        let t = parse(SAMPLE);
        assert_eq!(t.entries[1].parents, ["en:vegetable"]);
        assert_eq!(t.entries[2].parents, ["en:white-cabbage"]);
        assert_eq!(t.entries[3].parents, ["en:vegetable"]);
        assert_eq!(
            t.unresolved,
            [("en:kimchi".to_owned(), "en: no such parent".to_owned())]
        );
    }

    #[test]
    fn self_and_repeated_parents_are_dropped() {
        let t = parse("en: root\n\n< en: root\n< en: root\n< en: leaf\nen: leaf\n");
        assert_eq!(t.entries[1].parents, ["en:root"]);
        assert!(t.unresolved.is_empty());
    }

    #[test]
    fn a_whitespace_only_line_separates_blocks() {
        let t = parse("en: one\r\n   \r\nen: two\n");
        let ids: Vec<&str> = t.entries.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["en:one", "en:two"]);
    }

    #[test]
    fn comment_lines_never_become_names_or_properties() {
        let t = parse("# en: ghost\nen: real\n# vegan:en: no\n");
        assert_eq!(t.entries.len(), 1);
        assert_eq!(t.entries[0].names["en"], ["real"]);
        assert!(t.entries[0].props.is_empty());
    }

    #[test]
    fn language_codes_are_two_or_three_lowercase_letters() {
        for yes in ["en", "yue", "zh_yue"] {
            assert!(is_language(yes), "{yes}");
        }
        for no in ["e", "engl", "EN", "e1", "zh_", "zh_Yue", "_en"] {
            assert!(!is_language(no), "{no}");
        }
    }

    #[test]
    fn canonical_ids_fold_case_and_spacing() {
        assert_eq!(canonical("en", "  Fish  Sauce "), "en:fish-sauce");
        assert_eq!(canonical("fr", "Crème_fraîche"), "fr:crème-fraîche");
        assert!(is_language("zh_yue") && is_language("en") && !is_language("vegan:en"));
    }
}
