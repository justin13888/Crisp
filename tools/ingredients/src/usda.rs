//! Densities from USDA FoodData Central (SR Legacy) portion weights.
//!
//! Each food's volume portions (`1 cup`, `1 tbsp, chopped`) give grams per
//! millilitre; a food's density is the median over its plain portions, or
//! over all volume portions when it has no plain one. Foods are then matched
//! to index entries as candidates of a *kind* — `linked` (the taxonomy
//! names the FDC id), `exact` (one-segment description equal to a name),
//! `fuzzy` (same words) — and the rules accept whole kinds.

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;

use eyre::{Context, Result, eyre};
use serde::Serialize;

/// One FDC food with a derivable density.
#[derive(Debug, Clone, PartialEq)]
pub struct Food {
    /// The FDC id.
    pub fdc_id: String,
    /// The FDC description, e.g. `Onions, raw`.
    pub description: String,
    /// Grams per millilitre.
    pub density: f64,
}

/// A proposed entry ↔ food match, as written to `usda-map.json`.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Candidate {
    /// The index slug.
    pub slug: String,
    /// The FDC id.
    pub fdc_id: String,
    /// The FDC description.
    pub description: String,
    /// `linked`, `exact`, or `fuzzy`.
    pub kind: String,
    /// Grams per millilitre.
    pub density: f64,
    /// Whether the rules accept this kind.
    pub accepted: bool,
}

/// Reads one CSV member of the archive into rows of named fields.
fn read_csv(
    archive: &mut zip::ZipArchive<std::fs::File>,
    suffix: &str,
) -> Result<Vec<BTreeMap<String, String>>> {
    let mut name = None;
    for candidate in archive.file_names() {
        let candidate = candidate?;
        if candidate.ends_with(suffix) {
            name = Some(candidate.into_owned());
            break;
        }
    }
    let name = name.ok_or_else(|| eyre!("the FDC archive has no {suffix}"))?;
    let mut text = String::new();
    archive
        .by_name(&name)?
        .read_to_string(&mut text)
        .wrap_err_with(|| format!("reading {name}"))?;
    let mut reader = csv::Reader::from_reader(text.as_bytes());
    let headers = reader.headers()?.clone();
    let mut rows = Vec::new();
    for record in reader.records() {
        let record = record?;
        rows.push(
            headers
                .iter()
                .zip(record.iter())
                .map(|(h, v)| (h.to_owned(), v.to_owned()))
                .collect(),
        );
    }
    Ok(rows)
}

/// The median of a non-empty list.
pub fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f64::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len().is_multiple_of(2) {
        f64::midpoint(values[mid - 1], values[mid])
    } else {
        values[mid]
    })
}

/// Derives densities from `food.csv` and `food_portion.csv` rows.
#[must_use]
pub fn densities(
    foods: &[BTreeMap<String, String>],
    portions: &[BTreeMap<String, String>],
    volume_units: &BTreeMap<String, f64>,
) -> Vec<Food> {
    let mut plain: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    let mut any: BTreeMap<&str, Vec<f64>> = BTreeMap::new();
    for p in portions {
        let field = |k: &str| p.get(k).map_or("", String::as_str);
        let modifier = field("modifier").trim().to_lowercase();
        let (unit, qualifier) = modifier
            .split_once(',')
            .map_or((modifier.as_str(), ""), |(u, q)| (u.trim(), q.trim()));
        let Some(ml) = volume_units.get(unit) else {
            continue;
        };
        let (Ok(amount), Ok(grams)) = (
            field("amount").parse::<f64>(),
            field("gram_weight").parse::<f64>(),
        ) else {
            continue;
        };
        // A zero or negative amount or weight falls outside the plausible
        // range below (or divides to infinity), so it needs no check of its own.
        let density = grams / (amount * ml);
        if !(0.05..=3.0).contains(&density) {
            continue;
        }
        let id = field("fdc_id");
        any.entry(id).or_default().push(density);
        if qualifier.is_empty() {
            plain.entry(id).or_default().push(density);
        }
    }
    let mut out = Vec::new();
    for food in foods {
        let Some(id) = food.get("fdc_id") else {
            continue;
        };
        let values = plain.get(id.as_str()).or_else(|| any.get(id.as_str()));
        if let Some(density) = values.and_then(|v| median(v.clone())) {
            out.push(Food {
                fdc_id: id.clone(),
                description: food.get("description").cloned().unwrap_or_default(),
                density,
            });
        }
    }
    out
}

/// Loads FDC foods with densities from the archive.
///
/// # Errors
/// When the archive cannot be opened or lacks the expected CSV files.
pub fn load(path: &Path, volume_units: &BTreeMap<String, f64>) -> Result<Vec<Food>> {
    let file = std::fs::File::open(path).wrap_err_with(|| format!("opening {}", path.display()))?;
    let mut archive = zip::ZipArchive::new(file)?;
    let foods = read_csv(&mut archive, "/food.csv")?;
    let portions = read_csv(&mut archive, "/food_portion.csv")?;
    Ok(densities(&foods, &portions, volume_units))
}

/// Lowercase words, a trailing plural `s` dropped, `raw` ignored.
fn words(text: &str) -> BTreeSet<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty() && *w != "raw")
        .map(|w| {
            w.strip_suffix('s')
                .filter(|s| s.len() > 2)
                .unwrap_or(w)
                .to_owned()
        })
        .collect()
}

/// One entry's matchable facts.
pub struct Matchable<'a> {
    /// The index slug.
    pub slug: &'a str,
    /// FDC ids the taxonomy links to it.
    pub linked: Vec<&'a str>,
    /// Its English names.
    pub names: Vec<&'a str>,
}

/// Proposes candidates for every entry, best kind per (entry, food) pair.
#[must_use]
pub fn candidates(entries: &[Matchable<'_>], foods: &[Food], accept: &[String]) -> Vec<Candidate> {
    let by_id: BTreeMap<&str, &Food> = foods.iter().map(|f| (f.fdc_id.as_str(), f)).collect();
    let mut by_single: BTreeMap<BTreeSet<String>, Vec<&Food>> = BTreeMap::new();
    let mut by_words: BTreeMap<BTreeSet<String>, Vec<&Food>> = BTreeMap::new();
    for food in foods {
        let segments: Vec<&str> = food
            .description
            .split(',')
            .map(str::trim)
            .filter(|s| !s.eq_ignore_ascii_case("raw"))
            .collect();
        if segments.len() == 1 {
            by_single.entry(words(segments[0])).or_default().push(food);
        }
        by_words
            .entry(words(&food.description))
            .or_default()
            .push(food);
    }
    let mut out = Vec::new();
    for entry in entries {
        let mut seen = BTreeSet::new();
        let mut push = |food: &Food, kind: &str| {
            if seen.insert(food.fdc_id.clone()) {
                out.push(Candidate {
                    slug: entry.slug.to_owned(),
                    fdc_id: food.fdc_id.clone(),
                    description: food.description.clone(),
                    kind: kind.to_owned(),
                    density: (food.density * 1000.0).round() / 1000.0,
                    accepted: accept.iter().any(|a| a == kind),
                });
            }
        };
        for id in &entry.linked {
            if let Some(food) = by_id.get(id) {
                push(food, "linked");
            }
        }
        for name in &entry.names {
            for food in by_single.get(&words(name)).into_iter().flatten() {
                push(food, "exact");
            }
        }
        for name in &entry.names {
            for food in by_words.get(&words(name)).into_iter().flatten() {
                push(food, "fuzzy");
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect()
    }

    fn units() -> BTreeMap<String, f64> {
        BTreeMap::from([("cup".to_owned(), 240.0), ("tbsp".to_owned(), 15.0)])
    }

    #[test]
    fn densities_prefer_plain_portions_and_drop_implausible_ones() {
        let foods = vec![
            row(&[("fdc_id", "1"), ("description", "Onions, raw")]),
            row(&[("fdc_id", "2"), ("description", "Spices, pepper, black")]),
            row(&[("fdc_id", "3"), ("description", "Mystery")]),
        ];
        let portions = vec![
            row(&[
                ("fdc_id", "1"),
                ("amount", "1"),
                ("modifier", "cup, chopped"),
                ("gram_weight", "160"),
            ]),
            row(&[
                ("fdc_id", "1"),
                ("amount", "1"),
                ("modifier", "cup"),
                ("gram_weight", "120"),
            ]),
            row(&[
                ("fdc_id", "2"),
                ("amount", "1"),
                ("modifier", "tbsp, ground"),
                ("gram_weight", "6"),
            ]),
            row(&[
                ("fdc_id", "3"),
                ("amount", "1"),
                ("modifier", "cup"),
                ("gram_weight", "2000"),
            ]),
            row(&[
                ("fdc_id", "3"),
                ("amount", "1"),
                ("modifier", "serving"),
                ("gram_weight", "20"),
            ]),
        ];
        let got = densities(&foods, &portions, &units());
        assert_eq!(got.len(), 2);
        assert!((got[0].density - 0.5).abs() < 1e-9);
        assert!((got[1].density - 0.4).abs() < 1e-9);
        assert_eq!(median(vec![3.0, 1.0, 2.0, 4.0]), Some(2.5));
        assert_eq!(median(Vec::new()), None);
    }

    #[test]
    fn words_drop_plurals_but_not_short_words() {
        let got: Vec<String> = words("Gas, Onions, raw").into_iter().collect();
        assert_eq!(got, ["gas", "onion"]);
    }

    #[test]
    fn archives_load_from_their_csv_members() {
        use std::io::Write as _;
        let path = std::env::temp_dir().join(format!("crisp-fdc-{}.zip", std::process::id()));
        {
            let file = std::fs::File::create(&path).expect("create");
            let mut zip = zip::ZipWriter::new(file);
            let options = zip::write::SimpleFileOptions::default();
            zip.start_file("sr/food.csv", options).expect("food");
            zip.write_all(b"\"fdc_id\",\"description\"\n\"1\",\"Milk, whole\"\n")
                .expect("write");
            zip.start_file("sr/food_portion.csv", options)
                .expect("portion");
            zip.write_all(b"fdc_id,amount,modifier,gram_weight\n1,1,cup,240\n")
                .expect("write");
            zip.finish().expect("finish");
        }
        let foods = load(&path, &units()).expect("load");
        std::fs::remove_file(&path).ok();
        assert_eq!(foods.len(), 1);
        assert_eq!(foods[0].description, "Milk, whole");
        assert!((foods[0].density - 1.0).abs() < 1e-9);
        assert!(load(Path::new("/nonexistent/fdc.zip"), &units()).is_err());
    }

    #[test]
    fn candidate_densities_round_to_thousandths() {
        let foods = vec![Food {
            fdc_id: "1".to_owned(),
            description: "Onions".to_owned(),
            density: 0.123_456,
        }];
        let entries = vec![Matchable {
            slug: "onion",
            linked: vec![],
            names: vec!["onion"],
        }];
        let got = candidates(&entries, &foods, &[]);
        assert!((got[0].density - 0.123).abs() < 1e-12, "{}", got[0].density);
        assert!(!got[0].accepted);
    }

    #[test]
    fn candidates_carry_their_kind_and_acceptance() {
        let foods = vec![
            Food {
                fdc_id: "1".to_owned(),
                description: "Onions, raw".to_owned(),
                density: 0.5,
            },
            Food {
                fdc_id: "2".to_owned(),
                description: "Spices, pepper, black".to_owned(),
                density: 0.4,
            },
            Food {
                fdc_id: "9".to_owned(),
                description: "Oil, sesame".to_owned(),
                density: 0.9,
            },
        ];
        let entries = vec![
            Matchable {
                slug: "onion",
                linked: vec![],
                names: vec!["onion"],
            },
            Matchable {
                slug: "black-pepper",
                linked: vec![],
                names: vec!["pepper black spices"],
            },
            Matchable {
                slug: "sesame-oil",
                linked: vec!["9", "404"],
                names: vec!["sesame oil"],
            },
        ];
        let accept = vec!["linked".to_owned(), "exact".to_owned()];
        let got: Vec<(String, String, bool)> = candidates(&entries, &foods, &accept)
            .into_iter()
            .map(|c| (c.slug, c.kind, c.accepted))
            .collect();
        let want = [
            ("onion", "exact", true),
            ("black-pepper", "fuzzy", false),
            ("sesame-oil", "linked", true),
        ]
        .map(|(s, k, a)| (s.to_owned(), k.to_owned(), a));
        assert_eq!(got, want);
    }
}
