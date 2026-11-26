//! The ingredient index's suggestion intake (`docs/ingredient-index.md` §6).
//!
//! Signed-in accounts propose index changes; this server's administrators
//! triage them; accepted ones leave as override patches for the repository,
//! where a maintainer reviews them. Nothing here changes the index this
//! server embeds: it changes only with a release.
//!
//! - [`suggestions`] — the suggestion's shape, its validation against the
//!   embedded index, and `POST /ingredients/suggestions`.
//! - [`triage`] — the administrators' queue, triage, and export routes.
//! - [`export`] — rendering accepted suggestions as override patches.
//! - [`overrides`] — the committed override files the patches merge into.

pub mod export;
pub mod overrides;
pub mod suggestions;
pub mod triage;
