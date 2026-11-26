//! [`MockStorage`]'s ingredient-suggestion half: the suggestion queue and the
//! audit row triage appends, over the same vectors as the moderation half.

use async_trait::async_trait;
use crisp_server::db::{ingredient_suggestion, moderation_action};
use crisp_server::storage::{
    FileSuggestion, NewSuggestion, ResolveSuggestion, Suggestions, action, suggestion_target,
};
use sea_orm::DbErr;

use super::mock::{Inner, MockStorage};

/// Clears `usr_id` from the reports and suggestions it filed, as the
/// schema's `ON DELETE SET NULL` does when its account is deleted.
pub(super) fn orphan(inner: &mut Inner, usr_id: &str) {
    for report in &mut inner.reports {
        if report.reporter.as_deref() == Some(usr_id) {
            report.reporter = None;
        }
    }
    for suggestion in &mut inner.suggestions {
        if suggestion.suggester.as_deref() == Some(usr_id) {
            suggestion.suggester = None;
        }
    }
}

#[async_trait]
impl Suggestions for MockStorage {
    async fn file_suggestion(
        &self,
        suggestion: NewSuggestion,
        max_open: u64,
    ) -> Result<FileSuggestion, DbErr> {
        self.check("file_suggestion")?;
        let mut inner = self.inner.lock().unwrap();
        let open = (inner.suggestions.iter())
            .filter(|s| s.suggester.as_deref() == Some(&suggestion.suggester))
            .filter(|s| s.status == "open")
            .count() as u64;
        if open >= max_open {
            return Ok(FileSuggestion::TooMany);
        }
        inner.next_id += 1;
        let row = ingredient_suggestion::Model {
            id: inner.next_id,
            suggester: Some(suggestion.suggester),
            kind: suggestion.kind,
            slug: suggestion.slug,
            locale: suggestion.locale,
            text: suggestion.text,
            note: suggestion.note,
            status: "open".to_owned(),
            created_at: suggestion.at,
            resolved_at: None,
            resolved_by: None,
        };
        inner.suggestions.push(row.clone());
        Ok(FileSuggestion::Filed(Box::new(row)))
    }

    async fn suggestions(
        &self,
        status: Option<&str>,
        limit: u64,
        offset: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr> {
        self.check("suggestions")?;
        let inner = self.inner.lock().unwrap();
        Ok((inner.suggestions.iter())
            .filter(|s| status.is_none_or(|status| s.status == status))
            .skip(offset as usize)
            .take(limit as usize)
            .cloned()
            .collect())
    }

    async fn accepted_suggestions(
        &self,
        since: i64,
        after: Option<i64>,
        limit: u64,
    ) -> Result<Vec<ingredient_suggestion::Model>, DbErr> {
        self.check("accepted_suggestions")?;
        let inner = self.inner.lock().unwrap();
        let in_window = |s: &ingredient_suggestion::Model| match (s.resolved_at, after) {
            (None, _) => false,
            (Some(at), None) => at >= since,
            (Some(at), Some(after)) => (at, s.id) > (since, after),
        };
        let mut rows: Vec<_> = (inner.suggestions.iter())
            .filter(|s| s.status == "accepted" && in_window(s))
            .cloned()
            .collect();
        rows.sort_by_key(|s| (s.resolved_at, s.id));
        rows.truncate(limit as usize);
        Ok(rows)
    }

    async fn resolve_suggestion(
        &self,
        id: i64,
        resolution: &str,
        actor: &str,
        at: i64,
    ) -> Result<ResolveSuggestion, DbErr> {
        self.check("resolve_suggestion")?;
        let mut inner = self.inner.lock().unwrap();
        let Some(found) = inner.suggestions.iter_mut().find(|s| s.id == id) else {
            return Ok(ResolveSuggestion::Absent);
        };
        if found.status != "open" {
            return Ok(ResolveSuggestion::AlreadyResolved(found.clone()));
        }
        found.status = resolution.to_owned();
        found.resolved_at = Some(at);
        found.resolved_by = Some(actor.to_owned());
        let suggestion = found.clone();
        inner.next_id += 1;
        let audit = moderation_action::Model {
            id: inner.next_id,
            actor: actor.to_owned(),
            action: action::RESOLVE_SUGGESTION.to_owned(),
            target: suggestion_target(id),
            reason: Some(resolution.to_owned()),
            report_id: None,
            created_at: at,
        };
        inner.actions.push(audit.clone());
        Ok(ResolveSuggestion::Resolved { suggestion, audit })
    }
}
