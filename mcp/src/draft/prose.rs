//! Draft prose: plain text whose blank lines separate paragraphs and whose
//! `[text](https://…)` spans become linked runs. No other markup exists in
//! the format, so none is accepted: emphasis markers are refused rather
//! than silently kept as literal asterisks.

use crisp_core::model::text::{Paragraph, Prose, Run};

/// Parses draft prose, or explains what is wrong with it.
pub(super) fn parse(text: &str) -> Result<Prose, String> {
    let text = text.replace("\r\n", "\n");
    if text.contains("**") || text.contains('`') {
        return Err(
            "formatting markup (** or `) is not supported: write plain text, with links as \
             [text](https://…)"
                .into(),
        );
    }
    let mut paragraphs = Vec::new();
    for block in split_paragraphs(&text) {
        paragraphs.push(Paragraph(runs(block)?));
    }
    if paragraphs.is_empty() {
        return Err("the text is empty".into());
    }
    Ok(Prose(paragraphs))
}

/// Splits on blank lines, trimming each paragraph and dropping empty ones.
fn split_paragraphs(text: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut offset = 0;
    let mut blank_run = false;
    for line in text.split_inclusive('\n') {
        let blank = line.trim().is_empty();
        if blank && !blank_run {
            out.push(&text[start..offset]);
        }
        if !blank && blank_run {
            start = offset;
        }
        blank_run = blank;
        offset += line.len();
    }
    if !blank_run {
        out.push(&text[start..]);
    }
    out.into_iter()
        .map(str::trim)
        .filter(|p| !p.is_empty())
        .collect()
}

/// Splits one paragraph into text and link runs.
fn runs(paragraph: &str) -> Result<Vec<Run>, String> {
    let mut runs = Vec::new();
    let mut rest = paragraph;
    while let Some((before, label, url, after)) = next_link(rest) {
        if !url.starts_with("https://") || url.len() <= "https://".len() {
            return Err(format!("link {url:?} must be an https:// URL"));
        }
        if !before.is_empty() {
            runs.push(text_run(before, None));
        }
        runs.push(text_run(label, Some(url)));
        rest = after;
    }
    if !rest.is_empty() {
        runs.push(text_run(rest, None));
    }
    Ok(runs)
}

/// Finds the next `[label](url)`: returns the text before it, the label,
/// the URL, and the text after it.
fn next_link(text: &str) -> Option<(&str, &str, &str, &str)> {
    let mut search = 0;
    while let Some(open) = text[search..].find('[').map(|i| i + search) {
        let close = text[open..].find("](").map(|i| i + open)?;
        let label = &text[open + 1..close];
        if label.contains('[') {
            search = open + 1;
            continue;
        }
        let url_start = close + 2;
        let end = text[url_start..].find(')').map(|i| i + url_start)?;
        let url = &text[url_start..end];
        if label.is_empty() || url.chars().any(char::is_whitespace) {
            search = open + 1;
            continue;
        }
        return Some((&text[..open], label, url, &text[end + 1..]));
    }
    None
}

fn text_run(text: &str, link: Option<&str>) -> Run {
    Run {
        t: text.to_owned(),
        style: None,
        link: link.map(str::to_owned),
        reference: None,
    }
}

/// Renders prose back to draft text. The flag is true when something the
/// draft cannot express (styles, internal references) was flattened.
pub(super) fn render(prose: &Prose) -> (String, bool) {
    let mut flattened = false;
    let paragraphs: Vec<String> = prose
        .0
        .iter()
        .map(|paragraph| {
            paragraph
                .0
                .iter()
                .map(|run| {
                    flattened |= run.style.as_ref().is_some_and(|s| !s.is_empty())
                        || run.reference.is_some();
                    match &run.link {
                        Some(link) => format!("[{}]({link})", run.t),
                        None => run.t.clone(),
                    }
                })
                .collect::<String>()
        })
        .collect();
    (paragraphs.join("\n\n"), flattened)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crisp_core::model::text::{RunRef, RunRefKind, RunTarget, StyleFlag};

    fn plain(prose: &Prose) -> Vec<Vec<(String, Option<String>)>> {
        prose
            .0
            .iter()
            .map(|p| p.0.iter().map(|r| (r.t.clone(), r.link.clone())).collect())
            .collect()
    }

    #[test]
    fn paragraphs_split_on_blank_lines_and_keep_line_breaks() {
        let prose = parse("First line\nsame paragraph.\r\n\r\n  \n\nSecond.\n").expect("parses");
        assert_eq!(
            plain(&prose),
            vec![
                vec![("First line\nsame paragraph.".to_owned(), None)],
                vec![("Second.".to_owned(), None)],
            ]
        );
    }

    #[test]
    fn links_become_linked_runs() {
        let prose = parse("See [the PDO spec](https://example.eu/spec) and [x](https://a.b) now.")
            .expect("parses");
        assert_eq!(
            plain(&prose),
            vec![vec![
                ("See ".to_owned(), None),
                (
                    "the PDO spec".to_owned(),
                    Some("https://example.eu/spec".to_owned())
                ),
                (" and ".to_owned(), None),
                ("x".to_owned(), Some("https://a.b".to_owned())),
                (" now.".to_owned(), None),
            ]]
        );
    }

    #[test]
    fn brackets_that_are_not_links_stay_text() {
        let prose = parse("Use [optional] salt [and (pepper)] [] (x) [a](has space)").expect("ok");
        assert_eq!(prose.0[0].0.len(), 1);
        assert!(parse("[unterminated](https://x").is_ok());
        assert!(parse("[label] no paren").is_ok());
    }

    #[test]
    fn insecure_links_markup_and_empty_text_are_refused() {
        assert!(
            parse("[x](http://insecure.example)")
                .expect_err("http")
                .contains("https")
        );
        assert!(parse("[x](https://)").is_err());
        assert!(
            parse("very **bold**")
                .expect_err("markup")
                .contains("markup")
        );
        assert!(parse("`code`").is_err());
        assert!(parse(" \n\n ").expect_err("empty").contains("empty"));
    }

    #[test]
    fn rendering_round_trips_links_and_reports_flattening() {
        let text = "One [link](https://x.y/z).\n\nTwo.";
        let (back, flattened) = render(&parse(text).expect("parses"));
        assert_eq!(back, text);
        assert!(!flattened);

        let mut styled = parse("Bold").expect("parses");
        styled.0[0].0[0].style = Some(vec![StyleFlag::Bold]);
        assert_eq!(render(&styled), ("Bold".to_owned(), true));
        let mut empty_style = parse("Plain").expect("parses");
        empty_style.0[0].0[0].style = Some(vec![]);
        assert!(!render(&empty_style).1);
        let mut referenced = parse("the butter").expect("parses");
        referenced.0[0].0[0].reference = Some(RunRef {
            kind: RunRefKind::Ingredient,
            target: RunTarget::Handle(1),
        });
        assert!(render(&referenced).1);
    }
}
