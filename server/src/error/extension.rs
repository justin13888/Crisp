//! The extension member types the problem kinds publish beyond RFC 9457's own
//! members. Each one is described in the OpenAPI document (a kynos
//! requirement for every `#[problem(extension)]` field) and converts into the
//! JSON value the problem body carries.

use crisp_core::verify::ComplianceReport;
use kynos::{
    openapi::{Schema as OpenApiSchema, model::schema::types::SchemaType},
    schema::{Schema, registry::Registry},
};

/// A problem's stable, machine-readable `code` member: one of the
/// [`code`](super::code) constants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProblemCode(pub &'static str);

impl Schema for ProblemCode {
    fn schema(_registry: &mut Registry) -> OpenApiSchema {
        OpenApiSchema::of_type(SchemaType::String)
    }
}

impl From<ProblemCode> for serde_json::Value {
    fn from(code: ProblemCode) -> Self {
        Self::from(code.0)
    }
}

/// A compliance report, as published in a
/// [`VerificationFailed`](super::VerificationFailed) problem.
///
/// Wraps `crisp-core`'s report so it can be rendered as a JSON extension
/// member without the core crate knowing about HTTP.
#[derive(Debug)]
pub struct ReportExtension(pub(super) Box<ComplianceReport>);

impl Schema for ReportExtension {
    /// An object: the report's members are specified by the Crisp format
    /// (`spec/README.md` §11.4), not restated here.
    fn schema(_registry: &mut Registry) -> OpenApiSchema {
        let mut schema = OpenApiSchema::of_type(SchemaType::Object);
        if let OpenApiSchema::Object(object) = &mut schema {
            object.description = Some(
                "The compliance report for the submitted bundle (spec/README.md §11.4).".to_owned(),
            );
        }
        schema
    }
}

impl From<ReportExtension> for serde_json::Value {
    fn from(report: ReportExtension) -> Self {
        serde_json::to_value(&*report.0).unwrap_or_else(|error| {
            tracing::error!(%error, "failed to serialize a compliance report");
            Self::Null
        })
    }
}
