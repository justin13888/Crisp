//! Reports (`docs/public-server-policy.md` §4.2): any account files one
//! against a document, or one revision; administrators list and close them.

use kynos::ParamValue;
use kynos::prelude::*;
use serde::{Deserialize, Serialize};

use super::{Category, log_action};
use crate::auth::{AdminSession, SessionToken};
use crate::db::report;
use crate::error::{BadRequest, Conflict, Internal, NotFound};
use crate::operation_error;
use crate::search::{DEFAULT_LIMIT, MAX_LIMIT};
use crate::state::Store;
use crate::storage::{FileReport, NewReport, Resolve};
use crate::util::now_epoch;

/// The most characters a report's detail may hold.
pub const MAX_DETAIL_CHARS: usize = 2000;
/// The most open reports one account may hold at once.
pub const MAX_OPEN_REPORTS: u64 = 20;

/// Where a report stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, ParamValue, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReportStatus {
    /// No administrator has resolved it yet.
    Open,
    /// Resolved with no action taken.
    Dismissed,
    /// Resolved by acting on it.
    Actioned,
}

impl ReportStatus {
    /// The status as stored.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Dismissed => "dismissed",
            Self::Actioned => "actioned",
        }
    }

    /// Reads a stored status back.
    ///
    /// # Errors
    ///
    /// Returns [`Internal`] for a value no version of the server writes.
    pub fn stored(value: &str) -> Result<Self, Internal> {
        [Self::Open, Self::Dismissed, Self::Actioned]
            .into_iter()
            .find(|status| status.as_str() == value)
            .ok_or_else(|| Internal::logged("an unknown stored report status", &value))
    }
}

/// How an administrator closes a report.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Schema, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Resolution {
    /// No action is warranted.
    Dismissed,
    /// The report was acted on (e.g. by a takedown).
    Actioned,
}

impl From<Resolution> for ReportStatus {
    fn from(resolution: Resolution) -> Self {
        match resolution {
            Resolution::Dismissed => Self::Dismissed,
            Resolution::Actioned => Self::Actioned,
        }
    }
}

/// The `POST /reports` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ReportRequest {
    /// The reported document id.
    pub doc_id: String,
    /// The reported revision, when the report concerns only one.
    pub revision: Option<String>,
    /// Why it is reported.
    pub category: Category,
    /// What is wrong, at most 2000 characters.
    pub detail: Option<String>,
}

/// A report, as filed and as administrators see it.
#[derive(Debug, Clone, PartialEq, Eq, Schema, Serialize, Deserialize)]
pub struct ReportView {
    /// The report id.
    pub id: i64,
    /// The reporting account, or `null` once it is deleted.
    pub reporter: Option<String>,
    /// The reported document id.
    pub doc_id: String,
    /// The reported revision, if the report names one.
    pub revision: Option<String>,
    /// Why it was reported.
    pub category: Category,
    /// The reporter's explanation (possibly empty).
    pub detail: String,
    /// Where the report stands.
    pub status: ReportStatus,
    /// When it was filed (epoch seconds).
    pub created_at: i64,
    /// When it was resolved (epoch seconds), if it was.
    pub resolved_at: Option<i64>,
    /// The administrator who resolved it, if one did.
    pub resolved_by: Option<String>,
}

impl TryFrom<report::Model> for ReportView {
    type Error = Internal;

    fn try_from(row: report::Model) -> Result<Self, Internal> {
        Ok(Self {
            id: row.id,
            reporter: row.reporter,
            doc_id: row.doc_id,
            revision: row.revision,
            category: Category::stored(&row.category)?,
            detail: row.detail,
            status: ReportStatus::stored(&row.status)?,
            created_at: row.created_at,
            resolved_at: row.resolved_at,
            resolved_by: row.resolved_by,
        })
    }
}

/// The `GET /admin/reports` response body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ReportList {
    /// The reports, oldest first.
    pub reports: Vec<ReportView>,
}

/// The filter and pagination of `GET /admin/reports`.
#[derive(Debug, Default, Schema, QueryParams)]
pub struct ReportQuery {
    /// Only reports with this status; every report when absent.
    pub status: Option<ReportStatus>,
    /// How many reports to return (default 50).
    #[schema(minimum = 1, maximum = 200)]
    pub limit: Option<u64>,
    /// How many reports to skip (default 0, at most 10 000).
    #[schema(maximum = 10000)]
    pub offset: Option<u64>,
}

/// What `POST /admin/reports/{id}/resolve` captures.
#[derive(Debug, Schema, PathParams)]
pub struct ReportPath {
    /// The report id.
    pub id: i64,
}

/// The `POST /admin/reports/{id}/resolve` request body.
#[derive(Debug, Schema, Serialize, Deserialize)]
pub struct ResolveRequest {
    /// How the report is closed.
    pub resolution: Resolution,
}

operation_error! {
    /// What `POST /reports` can fail with.
    pub enum FileReportError { BadRequest, NotFound, Conflict }
}

operation_error! {
    /// What `GET /admin/reports` can fail with.
    pub enum ListReportsError {}
}

operation_error! {
    /// What `POST /admin/reports/{id}/resolve` can fail with.
    pub enum ResolveReportError { NotFound, Conflict }
}

/// Checks a report's detail, trimmed, against [`MAX_DETAIL_CHARS`].
fn checked_detail(detail: Option<String>) -> Result<String, BadRequest> {
    let detail = detail.unwrap_or_default().trim().to_owned();
    let chars = detail.chars().count();
    if chars > MAX_DETAIL_CHARS {
        return Err(BadRequest::new(format!(
            "the detail is {chars} characters; at most {MAX_DETAIL_CHARS} are allowed"
        )));
    }
    Ok(detail)
}

/// Files a report against a published document, or one revision of it.
///
/// Refused 404 when nothing is published under that document (and
/// revision), and 409 while the caller already holds 20 open reports.
#[kynos::post("/reports")]
#[tracing::instrument(level = "info", skip_all, fields(usr_id, doc_id = %request.doc_id))]
pub async fn file_report(
    auth: Auth<SessionToken>,
    Inject(storage): Inject<Store>,
    Json(request): Json<ReportRequest>,
) -> Result<Json<ReportView>, FileReportError> {
    let caller = auth.into_inner().resolve()?;
    let reporter = caller.account.usr_id;
    tracing::Span::current().record("usr_id", reporter.as_str());
    let detail = checked_detail(request.detail)?;
    let published = match &request.revision {
        Some(revision) => storage.bundle_exact(&request.doc_id, revision).await?,
        None => storage.bundle_latest(&request.doc_id).await?,
    };
    if published.is_none() {
        tracing::debug!("report refused: nothing published there");
        return Err(NotFound::new("no bundle with that document id (and revision)").into());
    }
    let report = NewReport {
        reporter,
        doc_id: request.doc_id,
        revision: request.revision,
        category: request.category.as_str().to_owned(),
        detail,
        at: now_epoch(),
    };
    match storage.file_report(report, MAX_OPEN_REPORTS).await? {
        FileReport::Filed(row) => {
            tracing::info!(report_id = row.id, category = %row.category, "report filed");
            Ok(Json(row.try_into()?))
        }
        FileReport::TooMany => {
            tracing::info!("report refused: too many open reports");
            Err(Conflict::new(format!(
                "you already have {MAX_OPEN_REPORTS} open reports; wait for one to be resolved"
            ))
            .into())
        }
    }
}

/// Lists reports for administrators, oldest first, optionally only those
/// with one status (`?status=open` is the queue).
#[kynos::get("/admin/reports")]
#[tracing::instrument(level = "debug", skip_all)]
pub async fn list_reports(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Query(query): Query<ReportQuery>,
) -> Result<Json<ReportList>, ListReportsError> {
    let caller = admin.into_inner().resolve()?;
    let limit = query.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let status = query.status.map(ReportStatus::as_str);
    let rows = storage
        .reports(status, limit, query.offset.unwrap_or(0))
        .await?;
    tracing::debug!(actor = %caller.account.usr_id, ?status, rows = rows.len(), "listed reports");
    let reports = rows
        .into_iter()
        .map(ReportView::try_from)
        .collect::<Result<_, _>>()?;
    Ok(Json(ReportList { reports }))
}

/// Closes an open report as `dismissed` or `actioned`.
///
/// Refused 404 for an unknown report and 409 for one already resolved.
#[kynos::post("/admin/reports/{id}/resolve")]
#[tracing::instrument(level = "info", skip_all, fields(report_id = path.id))]
pub async fn resolve_report(
    admin: AdminSession,
    Inject(storage): Inject<Store>,
    Path(path): Path<ReportPath>,
    Json(request): Json<ResolveRequest>,
) -> Result<Json<ReportView>, ResolveReportError> {
    let caller = admin.into_inner().resolve()?;
    let resolution = ReportStatus::from(request.resolution).as_str();
    let outcome = storage
        .resolve_report(path.id, resolution, &caller.account.usr_id, now_epoch())
        .await?;
    match outcome {
        Resolve::Resolved { report, audit } => {
            log_action(&audit);
            Ok(Json(report.try_into()?))
        }
        Resolve::Absent => Err(NotFound::new("no report with that id").into()),
        Resolve::AlreadyResolved(report) => {
            Err(Conflict::new(format!("the report is already {}", report.status)).into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detail_is_trimmed_and_capped_in_characters() {
        assert_eq!(checked_detail(None).unwrap(), "");
        assert_eq!(checked_detail(Some("  spam  ".into())).unwrap(), "spam");
        let at_cap = "é".repeat(MAX_DETAIL_CHARS);
        assert_eq!(checked_detail(Some(at_cap.clone())).unwrap(), at_cap);
        assert!(checked_detail(Some("x".repeat(MAX_DETAIL_CHARS + 1))).is_err());
    }

    #[test]
    fn statuses_round_trip_and_resolutions_map_onto_them() {
        for status in [
            ReportStatus::Open,
            ReportStatus::Dismissed,
            ReportStatus::Actioned,
        ] {
            assert_eq!(ReportStatus::stored(status.as_str()).unwrap(), status);
        }
        assert!(ReportStatus::stored("closed").is_err());
        assert_eq!(
            ReportStatus::from(Resolution::Dismissed),
            ReportStatus::Dismissed
        );
        assert_eq!(
            ReportStatus::from(Resolution::Actioned),
            ReportStatus::Actioned
        );
    }
}
