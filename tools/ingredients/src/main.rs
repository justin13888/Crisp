//! `crisp-ingredients` — regenerates the Crisp ingredient index.
//!
//! ```text
//! crisp-ingredients sources    # print "<url> <cache file>" per pinned source
//! crisp-ingredients generate   # regenerate spec/ingredients and the report
//! crisp-ingredients check      # fail if the committed files are stale
//! ```
//!
//! Sources are read from `target/ingredients-cache/` (fetched by
//! `just ingredients`) and verified against the SHA-256 pins in
//! `tools/ingredients/rules/rules.toml` before anything is parsed. The
//! output is a pure function of the pinned sources, the rules, and the
//! previously published entries — see `docs/ingredient-index.md`.

mod assemble;
mod cli;
mod english;
mod names;
mod off;
mod opencc;
mod output;
mod rules;
mod select;
mod slug;
mod usda;

use eyre::Result;

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let command = std::env::args().nth(1).unwrap_or_default();
    cli::run(&command, &cli::Paths::repository(), &mut std::io::stdout())
}
