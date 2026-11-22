//! Chinese script and regional conversion with OpenCC dictionaries.
//!
//! A conversion is a chain of stages. Each stage merges one or more OpenCC
//! dictionaries (`key<TAB>value [alternatives…]`) and rewrites the text by
//! longest-forward-match, taking the first value; text no key matches is
//! kept. This is the core of OpenCC's `mmseg` segmentation-plus-conversion,
//! without its generated regional phrase table.

use std::collections::BTreeMap;

/// One merged dictionary stage.
#[derive(Debug, Clone, Default)]
pub struct Stage {
    map: BTreeMap<String, String>,
    longest: usize,
}

impl Stage {
    /// Merges dictionary texts; an earlier text's entry wins a shared key.
    #[must_use]
    pub fn from_texts(texts: &[&str]) -> Self {
        let mut stage = Self::default();
        for text in texts {
            for line in text.lines() {
                let Some((key, values)) = line.split_once('\t') else {
                    continue;
                };
                let Some(first) = values.split_whitespace().next() else {
                    continue;
                };
                if key.is_empty() {
                    continue;
                }
                stage.longest = stage.longest.max(key.chars().count());
                stage
                    .map
                    .entry(key.to_owned())
                    .or_insert_with(|| first.to_owned());
            }
        }
        stage
    }

    /// Rewrites `text` by longest-forward-match.
    #[must_use]
    pub fn apply(&self, text: &str) -> String {
        let chars: Vec<char> = text.chars().collect();
        let mut out = String::with_capacity(text.len());
        let mut rest: &[char] = &chars;
        while let Some(first) = rest.first() {
            let hit = (1..=self.longest.min(rest.len())).rev().find_map(|len| {
                let key: String = rest.iter().take(len).collect();
                self.map.get(&key).map(|v| (len, v))
            });
            let (len, value) = hit.map_or((1, None), |(len, v)| (len, Some(v)));
            match value {
                Some(value) => out.push_str(value),
                None => out.push(*first),
            }
            rest = rest.get(len..).unwrap_or_default();
        }
        out
    }
}

/// A conversion: stages applied in order.
#[derive(Debug, Clone, Default)]
pub struct Converter {
    stages: Vec<Stage>,
}

impl Converter {
    /// Builds a converter from its stages.
    #[must_use]
    pub fn new(stages: Vec<Stage>) -> Self {
        Self { stages }
    }

    /// Converts `text` through every stage.
    #[must_use]
    pub fn convert(&self, text: &str) -> String {
        self.stages
            .iter()
            .fold(text.to_owned(), |acc, stage| stage.apply(&acc))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn longest_match_wins_and_unknown_text_is_kept() {
        let phrases = "泡菜\t泡菜\n韩国\t韓國\n";
        let chars = "韩\t韓\n国\t國 国\n发\t發 髮\n";
        let st = Stage::from_texts(&[phrases, chars]);
        assert_eq!(st.apply("韩国泡菜"), "韓國泡菜");
        assert_eq!(st.apply("国发 kimchi"), "國發 kimchi");
    }

    #[test]
    fn stages_chain_and_earlier_dictionaries_win_shared_keys() {
        let st = Stage::from_texts(&["面\t麵\n", "面\t面\n"]);
        let tw = Stage::from_texts(&["麵\t麪\n"]);
        let conv = Converter::new(vec![st, tw]);
        assert_eq!(conv.convert("面条"), "麪条");
        assert_eq!(Converter::default().convert("面"), "面");
    }
}
