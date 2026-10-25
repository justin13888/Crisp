//! Parsing what a cook types ("1 1/2 cups", "2–3 tbsp", "1h 30m") into
//! quantities, and writing quantities back as editable text.

use serde::Serialize;

use crate::error::{CrispError, Result};
use crate::model::primitives::{LanguageTag, Quantity, Rational, UnitCode};
use crate::model::text::LocalizedText;
use crate::render::number::{add, multiply, reduce};
use crate::render::quantity::pick_text;
use crate::render::units::{UnitKind, label, lookup, resolve_alias};

/// The result of parsing a typed amount.
#[cfg_attr(feature = "ts", derive(tsify::Tsify))]
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParsedAmount {
    /// The parsed quantity.
    pub quantity: Quantity,
    /// Whether the unit came from the catalogue (`false` for annotations
    /// like `{clove}` or when there is no unit).
    pub unit_known: bool,
}

/// Parses a single number: `3`, `1.5`, `1,5`, `1/2`, `½`, `1½`, `1 1/2`.
/// Returns the value and the unparsed remainder.
fn parse_number(input: &str) -> Option<(Rational, &str)> {
    let s = input.trim_start();
    let digits_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    let (whole, rest) = s.split_at(digits_end);
    let glyph = |rest: &str| -> Option<(Rational, usize)> {
        let c = rest.chars().next()?;
        let (n, d) = match c {
            '½' => (1, 2),
            '⅓' => (1, 3),
            '⅔' => (2, 3),
            '¼' => (1, 4),
            '¾' => (3, 4),
            '⅕' => (1, 5),
            '⅖' => (2, 5),
            '⅗' => (3, 5),
            '⅘' => (4, 5),
            '⅙' => (1, 6),
            '⅚' => (5, 6),
            '⅛' => (1, 8),
            '⅜' => (3, 8),
            '⅝' => (5, 8),
            '⅞' => (7, 8),
            _ => return None,
        };
        Some((Rational::new(n, d), c.len_utf8()))
    };

    if whole.is_empty() {
        let (value, len) = glyph(s)?;
        return Some((value, &s[len..]));
    }
    let w: i64 = whole.parse().ok()?;
    // 1½
    if let Some((frac, len)) = glyph(rest) {
        let value = reduce(
            i128::from(w) * i128::from(frac.denominator) + i128::from(frac.numerator),
            u128::from(frac.denominator),
        )
        .ok()?;
        return Some((value, &rest[len..]));
    }
    // 1.5 / 1,5
    if let Some(after) = rest.strip_prefix('.').or_else(|| rest.strip_prefix(',')) {
        let frac_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        if frac_end > 0 && frac_end <= 6 {
            let (frac, rest) = after.split_at(frac_end);
            let scale = 10i128.pow(u32::try_from(frac_end).ok()?);
            let f: i128 = frac.parse().ok()?;
            let value = reduce(i128::from(w) * scale + f, u128::try_from(scale).ok()?).ok()?;
            return Some((value, rest));
        }
    }
    // 1/2
    if let Some(after) = rest.strip_prefix('/') {
        let den_end = after
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(after.len());
        let d: u64 = after[..den_end].parse().ok()?;
        if d == 0 {
            return None;
        }
        return Some((
            reduce(i128::from(w), u128::from(d)).ok()?,
            &after[den_end..],
        ));
    }
    // 1 1/2 or 1 ½
    let trimmed = rest.trim_start();
    if trimmed.len() < rest.len() {
        if let Some((frac, len)) = glyph(trimmed) {
            let value = reduce(
                i128::from(w) * i128::from(frac.denominator) + i128::from(frac.numerator),
                u128::from(frac.denominator),
            )
            .ok()?;
            return Some((value, &trimmed[len..]));
        }
        let num_end = trimmed
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(trimmed.len());
        if num_end > 0
            && let Some(after) = trimmed[num_end..].strip_prefix('/')
        {
            let n: i64 = trimmed[..num_end].parse().ok()?;
            let den_end = after
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(after.len());
            if let Ok(d) = after[..den_end].parse::<u64>()
                && d > 0
                && n < i64::try_from(d).unwrap_or(i64::MAX)
            {
                let value =
                    reduce(i128::from(w) * i128::from(d) + i128::from(n), u128::from(d)).ok()?;
                return Some((value, &after[den_end..]));
            }
        }
    }
    Some((Rational::new(w, 1), rest))
}

/// Strips a leading "about" marker, reporting whether one was present.
fn strip_approx(input: &str) -> (&str, bool) {
    let s = input.trim_start();
    for marker in ["~", "≈", "about ", "approx. ", "approx ", "ca. ", "around "] {
        if let Some(rest) = s.strip_prefix(marker) {
            return (rest, true);
        }
    }
    (s, false)
}

/// Strips a range separator (`-`, `–`, `—`, `to`) from the start of `input`.
fn strip_range(input: &str) -> Option<&str> {
    let s = input.trim_start();
    for sep in ["-", "–", "—", "to "] {
        if let Some(rest) = s.strip_prefix(sep) {
            return Some(rest);
        }
    }
    None
}

/// Whether `code` has the shape of a UCUM code: at most 64 bytes of
/// printable ASCII outside annotations, with `[…]`, `{…}`, and `(…)` closed
/// and properly nested, naming at least one letter or `%`.
///
/// This is a structural check, not a lookup: any code of this shape is
/// valid on the wire even when the catalogue cannot explain it. Annotation
/// text may hold any character but braces, so the annotations
/// [`parse_amount`] itself produces (`{fresh basil}`) pass.
fn is_ucum_shaped(code: &str) -> bool {
    if code.is_empty() || code.len() > 64 {
        return false;
    }
    let mut parens = 0usize;
    let mut in_brackets = false;
    let mut in_braces = false;
    let mut names_something = false;
    for c in code.chars() {
        if in_braces {
            match c {
                '}' => in_braces = false,
                '{' => return false,
                _ => names_something |= c.is_alphabetic(),
            }
            continue;
        }
        if !c.is_ascii_graphic() {
            return false;
        }
        names_something |= c.is_ascii_alphabetic() || c == '%';
        match c {
            '[' if in_brackets => return false,
            '[' => in_brackets = true,
            ']' if !in_brackets => return false,
            ']' => in_brackets = false,
            '{' if in_brackets => return false,
            '{' => in_braces = true,
            '}' => return false,
            '(' | ')' if in_brackets => return false,
            '(' => parens += 1,
            ')' if parens == 0 => return false,
            ')' => parens -= 1,
            _ => {}
        }
    }
    !in_brackets && !in_braces && parens == 0 && names_something
}

/// Reads a unit word typed after a number, returning the unit code (if any)
/// and whether the catalogue knows it.
///
/// In order: a catalogue alias or code (`cups` → `[cup_us]`); a cook's word
/// of letters, spaces, and hyphens, kept as a lowercase UCUM annotation
/// (`cloves` → `{cloves}`); then any other UCUM-shaped code, kept verbatim
/// (`[smoot]`, `{Clove}`, `kg/m2`). A code wrapped in one outer pair of
/// parentheses is taken without them, which is how [`quantity_to_input`]
/// writes a code that would otherwise read as a cook's word (`(mol)` →
/// `mol`); UCUM gives both the same meaning.
fn parse_unit(word: &str) -> Result<(Option<String>, bool)> {
    if word.is_empty() {
        return Ok((None, false));
    }
    if let Some(unit) = resolve_alias(word) {
        return Ok((Some(unit.code.to_owned()), true));
    }
    if word
        .chars()
        .all(|c| c.is_alphabetic() || c == ' ' || c == '-')
        && word.len() <= 62
    {
        return Ok((Some(format!("{{{}}}", word.to_lowercase())), false));
    }
    let code = word
        .strip_prefix('(')
        .and_then(|w| w.strip_suffix(')'))
        .filter(|inner| is_ucum_shaped(inner))
        .unwrap_or(word);
    if is_ucum_shaped(code) {
        return Ok((Some(code.to_owned()), lookup(code).is_some()));
    }
    Err(CrispError::input(format!("unrecognized unit {word:?}")))
}

/// Parses an amount a cook typed: a number or range, then an optional unit.
///
/// Anything that does not start with a number becomes a text amount in
/// `lang` ("to taste"). An unrecognized unit word becomes a UCUM annotation
/// (`3 cloves` → `{cloves}`), and any other UCUM code is kept verbatim
/// (`1 [smoot]`), since every UCUM code is valid on the wire.
///
/// # Errors
///
/// Returns [`CrispError::Input`] for empty input or a unit that is neither a
/// word nor shaped like a UCUM code.
pub fn parse_amount(input: &str, lang: &LanguageTag) -> Result<ParsedAmount> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err(CrispError::input("the amount is empty"));
    }
    let (body, approx) = strip_approx(trimmed);
    let Some((min, rest)) = parse_number(body) else {
        let text = LocalizedText([(lang.clone(), trimmed.to_owned())].into_iter().collect());
        return Ok(ParsedAmount {
            quantity: Quantity {
                text: Some(text),
                ..Quantity::default()
            },
            unit_known: false,
        });
    };
    let (max, rest) = match strip_range(rest).and_then(parse_number) {
        Some((max, rest)) => (Some(max), rest),
        None => (None, rest),
    };
    let (unit, unit_known) = parse_unit(rest.trim())?;
    Ok(ParsedAmount {
        quantity: Quantity {
            min: Some(min),
            max,
            unit: unit.map(UnitCode),
            approx: approx.then_some(true),
            text: None,
        },
        unit_known,
    })
}

/// Parses a duration: `90` (minutes), `90 min`, `1.5 h`, `1h 30m`,
/// `1 hr 30 min`, `1:30` (h:mm), or a range like `20-25 min`.
///
/// # Errors
///
/// Returns [`CrispError::Input`] if the text is not a duration.
pub fn parse_duration(input: &str) -> Result<Quantity> {
    let bad = || CrispError::input(format!("{:?} is not a duration", input.trim()));
    let s = input.trim();
    if s.is_empty() {
        return Err(bad());
    }
    // h:mm or h:mm:ss
    if s.contains(':') {
        let parts: Vec<&str> = s.split(':').collect();
        let nums: Option<Vec<u64>> = parts.iter().map(|p| p.trim().parse().ok()).collect();
        let nums = nums.ok_or_else(bad)?;
        let seconds = match nums.as_slice() {
            [h, m] if *m < 60 => h * 3600 + m * 60,
            [h, m, sec] if *m < 60 && *sec < 60 => h * 3600 + m * 60 + sec,
            _ => return Err(bad()),
        };
        return Ok(seconds_quantity(seconds));
    }
    // Bare number => minutes.
    if let Some((value, rest)) = parse_number(s)
        && rest.trim().is_empty()
    {
        return Ok(Quantity {
            min: Some(value),
            unit: Some(UnitCode("min".to_owned())),
            ..Quantity::default()
        });
    }
    // A single amount (possibly a range) in one time unit.
    if let Ok(parsed) = parse_amount(s, &LanguageTag("und".to_owned()))
        && parsed
            .quantity
            .unit
            .as_ref()
            .and_then(|u| lookup(&u.0))
            .is_some_and(|u| u.kind == UnitKind::Time)
    {
        return Ok(parsed.quantity);
    }
    // A sum of number+unit pairs: "1h 30m".
    let mut rest = s;
    let mut total = Rational::new(0, 1);
    let mut pairs = 0;
    while !rest.trim().is_empty() {
        let (value, after) = parse_number(rest).ok_or_else(bad)?;
        let after = after.trim_start();
        let word_end = after
            .find(|c: char| c.is_ascii_digit() || c.is_whitespace())
            .unwrap_or(after.len());
        let unit = resolve_alias(&after[..word_end])
            .filter(|u| u.kind == UnitKind::Time)
            .ok_or_else(bad)?;
        total = add(total, multiply(value, unit.factor)?)?;
        rest = &after[word_end..];
        pairs += 1;
    }
    if pairs == 0 {
        return Err(bad());
    }
    let seconds = u64::try_from(total.numerator)
        .map_err(|_| bad())?
        .div_ceil(total.denominator.max(1));
    Ok(seconds_quantity(seconds))
}

/// Expresses whole seconds in the largest unit that keeps them exact.
fn seconds_quantity(seconds: u64) -> Quantity {
    let (value, unit) = if seconds.is_multiple_of(60) {
        (seconds / 60, "min")
    } else {
        (seconds, "s")
    };
    Quantity {
        min: Some(Rational::new(i64::try_from(value).unwrap_or(i64::MAX), 1)),
        unit: Some(UnitCode(unit.to_owned())),
        ..Quantity::default()
    }
}

/// Writes a quantity back as editable ASCII-friendly text, the inverse of
/// [`parse_amount`] (`"1 1/2 cup"`, `"2-3 tbsp"`, `"to taste"`).
#[must_use]
pub fn quantity_to_input(quantity: &Quantity, lang: &LanguageTag) -> String {
    let Some(min) = quantity.min else {
        return quantity
            .text
            .as_ref()
            .map(|t| pick_text(t, lang).to_owned())
            .unwrap_or_default();
    };
    let number = |r: Rational| {
        let whole = r.numerator / i64::try_from(r.denominator).unwrap_or(1).max(1);
        let rem = r.numerator - whole * i64::try_from(r.denominator).unwrap_or(1);
        if rem == 0 {
            whole.to_string()
        } else if whole == 0 {
            format!("{rem}/{}", r.denominator)
        } else {
            format!("{whole} {}/{}", rem.abs(), r.denominator)
        }
    };
    let mut out = String::new();
    if quantity.approx == Some(true) {
        out.push('~');
    }
    out.push_str(&number(min));
    if let Some(max) = quantity.max {
        out.push('-');
        out.push_str(&number(max));
    }
    if let Some(unit) = &quantity.unit {
        out.push(' ');
        out.push_str(&unit_to_input(&unit.0));
    }
    out
}

/// Writes a unit code as text [`parse_unit`] reads back to the same code:
/// the friendliest form that round-trips (a catalogue alias, an
/// annotation's inner word), else the code verbatim, else the code in
/// parentheses so it is not read as a cook's word (`mol` → `(mol)`).
fn unit_to_input(code: &str) -> String {
    let friendly = match lookup(code) {
        Some(info) => info
            .aliases
            .first()
            .copied()
            .unwrap_or(info.code)
            .to_owned(),
        None => label(code, true),
    };
    // `parse_amount` trims the unit word, so a candidate is judged trimmed.
    let reads_back = |text: &str| matches!(parse_unit(text.trim()), Ok((Some(c), _)) if c == code);
    [friendly, code.to_owned(), format!("({code})")]
        .into_iter()
        .find(|text| reads_back(text))
        .unwrap_or_else(|| code.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::quantity::duration_seconds;
    use crate::render::units::UNITS;
    use proptest::prelude::*;

    fn en() -> LanguageTag {
        LanguageTag("en".to_owned())
    }

    fn q(n: i64, d: u64, unit: &str) -> Quantity {
        Quantity {
            min: Some(Rational::new(n, d)),
            unit: Some(UnitCode(unit.to_owned())),
            ..Quantity::default()
        }
    }

    #[test]
    fn parses_what_cooks_type() {
        let p = |s: &str| parse_amount(s, &en()).expect(s).quantity;
        assert_eq!(p("1 1/2 cups"), q(3, 2, "[cup_us]"));
        assert_eq!(p("1½ cups"), q(3, 2, "[cup_us]"));
        assert_eq!(p("½ tsp"), q(1, 2, "[tsp_us]"));
        assert_eq!(p("1.5 kg"), q(3, 2, "kg"));
        assert_eq!(p("1,5 L"), q(3, 2, "L"));
        assert_eq!(p("3 cloves"), q(3, 1, "{cloves}"));
        let range = p("2-3 Tbsp");
        assert_eq!(range.min, Some(Rational::new(2, 1)));
        assert_eq!(range.max, Some(Rational::new(3, 1)));
        assert_eq!(p("~200 g").approx, Some(true));
        assert!(p("to taste").text.is_some());
        assert_eq!(p("2").unit, None);
        assert!(parse_amount("", &en()).is_err());
        assert!(parse_amount("2 @#!", &en()).is_err());
        assert!(parse_amount("2 [smoot", &en()).is_err());
        assert!(parse_amount("2 {a{b}}", &en()).is_err());
    }

    #[test]
    fn keeps_uncatalogued_ucum_codes_verbatim() {
        let p = |s: &str| parse_amount(s, &en()).expect(s);
        let smoot = p("1 [smoot]");
        assert_eq!(smoot.quantity, q(1, 1, "[smoot]"));
        assert!(!smoot.unit_known);
        assert_eq!(p("2 kg/m2").quantity, q(2, 1, "kg/m2"));
        assert_eq!(p("2 {Clove}").quantity, q(2, 1, "{Clove}"));
        assert_eq!(p("2 (mol)").quantity, q(2, 1, "mol"));
        assert_eq!(p("2 mol").quantity, q(2, 1, "{mol}"));
    }

    #[test]
    fn every_unit_code_round_trips_through_the_editor() {
        let catalogue = UNITS.iter().map(|u| u.code);
        let annotations = [
            "{cloves}",
            "{Clove}",
            "{cup}",
            "{fresh basil}",
            "{größe}",
            "{clove_2}",
        ];
        let uncatalogued = ["[smoot]", "mol", "Hz", "kg/m2", "10*3{cells}", "(g)"];
        let amounts = [
            (Rational::new(1, 1), None, None),
            (Rational::new(3, 2), None, None),
            (Rational::new(1, 3), Some(Rational::new(1, 2)), None),
            (Rational::new(2, 1), Some(Rational::new(3, 1)), Some(true)),
        ];
        let mut cases = 0;
        for code in catalogue.chain(annotations).chain(uncatalogued) {
            for (min, max, approx) in amounts {
                let quantity = Quantity {
                    min: Some(min),
                    max,
                    unit: Some(UnitCode(code.to_owned())),
                    approx,
                    text: None,
                };
                let text = quantity_to_input(&quantity, &en());
                let back = parse_amount(&text, &en())
                    .unwrap_or_else(|e| panic!("{code}: {text:?} rejected: {e}"));
                assert_eq!(back.quantity, quantity, "{code}: {text:?}");
                assert_eq!(back.unit_known, lookup(code).is_some(), "{code}");
                cases += 1;
            }
        }
        assert!(cases > 100, "only {cases} cases");
    }

    /// A unit code from each class the wire allows: the catalogue, UCUM
    /// annotations, and uncatalogued codes.
    fn unit_code() -> impl Strategy<Value = String> {
        prop_oneof![
            prop::sample::select(UNITS.iter().map(|u| u.code.to_owned()).collect::<Vec<_>>()),
            "\\{[A-Za-z][A-Za-z _-]{0,20}\\}",
            "\\[[a-z][a-z_]{0,12}\\]",
            "[A-Za-z]{1,6}",
            "[a-z]{1,3}/[a-z]{1,3}[0-9]",
            "\\([a-z]{1,4}\\)",
        ]
    }

    /// A reduced, non-negative rational, the form the parser produces.
    fn amount() -> impl Strategy<Value = Rational> {
        (0i128..10_000, 1u128..16).prop_map(|(n, d)| reduce(n, d).expect("in range"))
    }

    proptest! {
        #[test]
        fn any_unit_code_survives_the_editor_round_trip(
            code in unit_code(),
            min in amount(),
            max in prop::option::of(amount()),
            approx in prop::option::of(Just(true)),
        ) {
            let quantity = Quantity {
                min: Some(min),
                max,
                unit: Some(UnitCode(code.clone())),
                approx,
                text: None,
            };
            let text = quantity_to_input(&quantity, &en());
            let back = parse_amount(&text, &en());
            prop_assert!(back.is_ok(), "{code}: {text:?} rejected");
            prop_assert_eq!(back.expect("checked").quantity, quantity, "{}: {:?}", code, text);
        }
    }

    #[test]
    fn parses_durations() {
        let mins = |n: i64| q(n, 1, "min");
        assert_eq!(parse_duration("90").unwrap(), mins(90));
        assert_eq!(parse_duration("1:30").unwrap(), mins(90));
        assert_eq!(parse_duration("1h 30m").unwrap(), mins(90));
        assert_eq!(parse_duration("1 hr 30 min").unwrap(), mins(90));
        assert_eq!(parse_duration("45 sec").unwrap(), q(45, 1, "s"));
        assert_eq!(parse_duration("1.5 h").unwrap(), q(3, 2, "h"));
        let range = parse_duration("20-25 min").unwrap();
        assert_eq!(range.max, Some(Rational::new(25, 1)));
        assert!(parse_duration("soon").is_err());
        assert_eq!(duration_seconds(&q(3, 2, "h")), Some((5400, None)));
    }

    #[test]
    fn input_text_round_trips_through_the_parser() {
        for text in [
            "1 1/2 cups",
            "2-3 tbsp",
            "200 g",
            "3 cloves",
            "1/2 tsp",
            "~2 cups",
        ] {
            let quantity = parse_amount(text, &en()).unwrap().quantity;
            let back = quantity_to_input(&quantity, &en());
            assert_eq!(
                parse_amount(&back, &en()).unwrap().quantity,
                quantity,
                "{text} -> {back}"
            );
        }
    }
}
