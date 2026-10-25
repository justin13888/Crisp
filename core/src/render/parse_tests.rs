//! Tests for the free-text amount and duration parsers and their inverse,
//! [`quantity_to_input`](super::quantity_to_input).

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
    assert_eq!(p("2 fresh basil").quantity, q(2, 1, "{fresh basil}"));
}

#[test]
fn ucum_shape_closes_and_nests_every_delimiter() {
    let longest = format!("[{}]", "a".repeat(62));
    for code in [
        "[smoot]",
        "kg/m2",
        "%",
        "{fresh basil}",
        "[a](b){c}",
        "(g)",
        longest.as_str(),
    ] {
        assert!(is_ucum_shaped(code), "{code} is UCUM-shaped");
    }
    let too_long = format!("[{}]", "a".repeat(63));
    for code in [
        "",
        "@#!",
        "{}",
        "a b",
        "[a[b]",
        "a]",
        "[a{b}]",
        "[a()]",
        "{a{b}",
        "a}",
        "a)",
        "(a",
        "[a",
        "{a",
        too_long.as_str(),
    ] {
        assert!(!is_ucum_shaped(code), "{code:?} is not UCUM-shaped");
    }
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
