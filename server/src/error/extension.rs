//! The extension member types the problem kinds publish beyond RFC 9457's own
//! members. Each one is described in the OpenAPI document (a kynos
//! requirement for every `#[problem(extension)]` field) and serializes as the
//! JSON value the problem body carries. The constant `code` member is not a
//! field: each kind declares it as `#[problem(extension(code = ...))]`.

use crisp_core::verify::ComplianceReport;
use kynos::{
    openapi::{Schema as OpenApiSchema, model::schema::types::SchemaType},
    schema::{Schema, registry::Registry},
};
use serde::{Serialize, Serializer};

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

impl Serialize for ReportExtension {
    /// The report's JSON form, or `null` (logged at ERROR) when it cannot be
    /// rendered, so the problem itself is still sent.
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serde_json::to_value(&*self.0)
            .unwrap_or_else(|error| {
                tracing::error!(%error, "failed to serialize a compliance report");
                serde_json::Value::Null
            })
            .serialize(serializer)
    }
}
