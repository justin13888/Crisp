//! The format engine for JavaScript: opening, verifying, and rendering
//! bundles, and the authoring helpers an editor needs.

use std::collections::BTreeMap;

use crisp_core::author::{new_identity, open_draft, validate_draft};
use crisp_core::identity::derive_blob_id;
use crisp_core::limits;
use crisp_core::model::blob::{Blob, MediaType};
use crisp_core::model::payload::{BundlePayload, Kind};
use crisp_core::model::primitives::{LanguageTag, OpenCode, Quantity};
use crisp_core::model::recipe::{AllergenCode, CourseCode, DietCode, NutrientCode};
use crisp_core::render::quantity::{format_quantity, quantity_to_input};
use crisp_core::render::units::UNITS;
use crisp_core::render::{ViewOptions, parse_amount, parse_duration, recipe_view, resolve_text};
use crisp_core::verify::{PlatformKey, verify};
use serde::{Deserialize, Serialize};
use tsify::Tsify;
use wasm_bindgen::prelude::*;

use crate::boundary::{OrJs, failure, from_js, timestamp, to_js};

/// Facts about this build of the engine.
#[derive(Serialize, Tsify)]
pub struct CoreInfo {
    /// The format version implemented, e.g. `65536` for 1.0.0.
    pub format_version: u64,
    /// The lowest `min_v` this build opens.
    pub min_reader_version: u64,
    /// The `crisp-core` crate version.
    pub core_version: String,
}

/// Returns facts about this build of the engine.
///
/// # Errors
///
/// Throws if serialization fails.
#[wasm_bindgen(js_name = coreInfo, unchecked_return_type = "CoreInfo")]
pub fn core_info() -> Result<JsValue, JsValue> {
    to_js(&CoreInfo {
        format_version: crisp_core::FORMAT_VERSION,
        min_reader_version: crisp_core::MIN_READER_VERSION,
        core_version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

/// The bounds and vocabularies an editor must respect.
#[derive(Serialize, Tsify)]
pub struct Constraints {
    /// Named numeric limits (spec §12 and the CDDL bounds).
    pub limits: BTreeMap<String, u64>,
    /// The registry codes for each open vocabulary.
    pub course: Vec<String>,
    /// Diet codes.
    pub diet: Vec<String>,
    /// Allergen codes.
    pub allergens: Vec<String>,
    /// Nutrient codes.
    pub nutrients: Vec<String>,
    /// Every catalogued unit, for unit pickers.
    pub units: Vec<crisp_core::render::units::UnitInfo>,
}

fn wire<T: Serialize + Clone>(values: &[T]) -> Vec<String> {
    values
        .iter()
        .map(|v| OpenCode::Known(v.clone()).as_wire())
        .collect()
}

/// Returns the bounds and vocabularies an editor must respect.
///
/// # Errors
///
/// Throws if serialization fails.
#[wasm_bindgen(js_name = recipeConstraints, unchecked_return_type = "Constraints")]
pub fn recipe_constraints() -> Result<JsValue, JsValue> {
    let limits: BTreeMap<String, u64> = [
        ("maxSteps", limits::MAX_STEPS),
        ("maxIngredients", limits::MAX_INGREDIENTS),
        ("maxEquipment", limits::MAX_EQUIPMENT),
        ("maxIngredientGroups", limits::MAX_INGREDIENT_GROUPS),
        ("maxStepGroups", limits::MAX_STEP_GROUPS),
        ("maxStepTimers", limits::MAX_STEP_TIMERS),
        ("maxAltQuantities", limits::MAX_ALT_QUANTITIES),
        ("maxSubstitutes", limits::MAX_SUBSTITUTES),
        ("maxTags", limits::MAX_TAGS),
        ("maxCuisine", limits::MAX_CUISINE),
        ("maxCourse", limits::MAX_COURSE),
        ("maxDiet", limits::MAX_DIET),
        ("maxAllergens", limits::MAX_ALLERGENS),
        ("maxMedia", limits::MAX_MEDIA),
        ("maxTextBytes", limits::MAX_TEXT_BYTES),
        ("maxTitleBytes", limits::MAX_TITLE_BYTES),
        ("maxBlobBytes", limits::MAX_BLOB_SIZE),
        ("maxBundleBytes", limits::MAX_BUNDLE_SIZE),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v as u64))
    .collect();
    to_js(&Constraints {
        limits,
        course: wire(CourseCode::ALL),
        diet: wire(DietCode::ALL),
        allergens: wire(AllergenCode::ALL),
        nutrients: wire(NutrientCode::ALL),
        units: UNITS.to_vec(),
    })
}

/// One recipe in a bundle summary.
#[derive(Serialize, Tsify)]
pub struct RecipeEntry {
    /// The index into the bundle's recipes.
    pub index: usize,
    /// The recipe id.
    pub id: String,
    /// The revision id.
    pub revision: String,
    /// The resolved title.
    pub title: String,
    /// The cover image blob id, if any.
    pub cover: Option<String>,
}

/// What a library list needs to know about a bundle.
#[derive(Serialize, Tsify)]
pub struct BundleSummary {
    /// The document id.
    pub doc_id: String,
    /// `"recipe"` or `"book"`.
    pub kind: String,
    /// The resolved document title.
    pub title: String,
    /// The recipes.
    pub recipes: Vec<RecipeEntry>,
    /// Author display names.
    pub authors: Vec<String>,
    /// Author ids, matching `authors`.
    pub author_ids: Vec<String>,
    /// Last modification (epoch seconds).
    pub modified_at: u64,
    /// The bundle's languages.
    pub languages: Vec<String>,
}

/// A blob's media type and bytes.
#[derive(Serialize, Tsify)]
pub struct BlobData {
    /// The media type, e.g. `image/webp`.
    pub media_type: String,
    /// The image bytes.
    #[serde(with = "serde_bytes_compat")]
    #[tsify(type = "Uint8Array")]
    pub bytes: Vec<u8>,
}

/// Serializes `Vec<u8>` as a byte string (a `Uint8Array` in JS).
mod serde_bytes_compat {
    use serde::Serializer;

    pub(super) fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(bytes)
    }
}

/// An opened `.crisp` file. Decoding is strict but does not verify; call
/// [`Bundle::verify`] for a compliance report.
#[wasm_bindgen]
pub struct Bundle {
    bytes: Vec<u8>,
    payload: BundlePayload,
}

#[wasm_bindgen]
impl Bundle {
    /// Opens `.crisp` file bytes.
    ///
    /// # Errors
    ///
    /// Throws (with a stable `code`) if the file cannot be decoded.
    pub fn open(bytes: &[u8]) -> Result<Bundle, JsValue> {
        let payload = open_draft(bytes).or_js()?;
        Ok(Self {
            bytes: bytes.to_vec(),
            payload,
        })
    }

    /// The file bytes.
    #[must_use]
    pub fn bytes(&self) -> Vec<u8> {
        self.bytes.clone()
    }

    /// The document id.
    #[wasm_bindgen(getter, js_name = docId)]
    #[must_use]
    pub fn doc_id(&self) -> String {
        self.payload.doc_id.0.clone()
    }

    /// Verifies the file against `platformKeys` (pass `[]` offline).
    ///
    /// # Errors
    ///
    /// Throws if the arguments are malformed.
    #[wasm_bindgen(unchecked_return_type = "ComplianceReport")]
    pub fn verify(
        &self,
        #[wasm_bindgen(js_name = platformKeys, unchecked_param_type = "PlatformKey[]")]
        platform_keys: JsValue,
        now: f64,
    ) -> Result<JsValue, JsValue> {
        let keys: Vec<PlatformKey> = from_js(platform_keys, "platform keys")?;
        to_js(&verify(&self.bytes, &keys, timestamp(now)?))
    }

    /// Summarizes the bundle in the first of `languages` it has.
    ///
    /// # Errors
    ///
    /// Throws if serialization fails.
    #[wasm_bindgen(unchecked_return_type = "BundleSummary")]
    pub fn summary(&self, languages: Vec<String>) -> Result<JsValue, JsValue> {
        let preferred: Vec<LanguageTag> = languages.into_iter().map(LanguageTag).collect();
        let default = &self.payload.default_language;
        let text = |t| {
            resolve_text(t, &preferred, default)
                .unwrap_or_default()
                .to_owned()
        };
        let p = &self.payload;
        to_js(&BundleSummary {
            doc_id: p.doc_id.0.clone(),
            kind: match p.kind {
                Kind::Recipe => "recipe",
                Kind::Book => "book",
            }
            .to_owned(),
            title: text(&p.meta.title),
            recipes: p
                .recipes
                .iter()
                .enumerate()
                .map(|(index, r)| RecipeEntry {
                    index,
                    id: r.id.0.clone(),
                    revision: r.revision.0.clone(),
                    title: text(&r.title),
                    cover: r
                        .media
                        .as_ref()
                        .and_then(|m| m.first())
                        .map(|b| b.0.clone()),
                })
                .collect(),
            authors: p
                .meta
                .authors
                .iter()
                .map(|a| a.display_name.clone())
                .collect(),
            author_ids: p.meta.authors.iter().map(|a| a.uid.0.clone()).collect(),
            modified_at: p.meta.modified_at.0,
            languages: p.languages.iter().map(|l| l.0.clone()).collect(),
        })
    }

    /// Builds the render-ready view of recipe `index`.
    ///
    /// # Errors
    ///
    /// Throws if `index` is out of range or the options are malformed.
    #[wasm_bindgen(unchecked_return_type = "RecipeView")]
    pub fn view(
        &self,
        index: usize,
        #[wasm_bindgen(unchecked_param_type = "ViewOptions")] options: JsValue,
    ) -> Result<JsValue, JsValue> {
        let options: ViewOptions = from_js(options, "view options")?;
        to_js(&recipe_view(&self.payload, index, &options).or_js()?)
    }

    /// The payload as an editable draft.
    ///
    /// # Errors
    ///
    /// Throws if serialization fails.
    #[wasm_bindgen(unchecked_return_type = "BundlePayload")]
    pub fn draft(&self) -> Result<JsValue, JsValue> {
        to_js(&self.payload)
    }

    /// A blob's media type and bytes, or `undefined`.
    ///
    /// # Errors
    ///
    /// Throws if serialization fails.
    #[wasm_bindgen(unchecked_return_type = "BlobData | undefined")]
    pub fn blob(&self, id: &str) -> Result<JsValue, JsValue> {
        match self.payload.blobs.iter().find(|(k, _)| k.0 == id) {
            Some((_, blob)) => to_js(&BlobData {
                media_type: blob.media_type.as_wire(),
                bytes: blob.bytes.clone(),
            }),
            None => Ok(JsValue::UNDEFINED),
        }
    }
}

/// Parses a typed amount (`"1 1/2 cups"`, `"to taste"`) in `lang`.
///
/// # Errors
///
/// Throws if the text is empty or its unit is unparseable.
#[wasm_bindgen(js_name = parseAmount, unchecked_return_type = "ParsedAmount")]
pub fn parse_amount_js(text: &str, lang: &str) -> Result<JsValue, JsValue> {
    to_js(&parse_amount(text, &LanguageTag(lang.to_owned())).or_js()?)
}

/// Parses a typed duration (`"90"`, `"1h 30m"`, `"20-25 min"`).
///
/// # Errors
///
/// Throws if the text is not a duration.
#[wasm_bindgen(js_name = parseDuration, unchecked_return_type = "Quantity")]
pub fn parse_duration_js(text: &str) -> Result<JsValue, JsValue> {
    to_js(&parse_duration(text).or_js()?)
}

/// Formats a quantity for display (`"1½ cups"`).
///
/// # Errors
///
/// Throws if the quantity is malformed.
#[wasm_bindgen(js_name = formatQuantity)]
pub fn format_quantity_js(
    #[wasm_bindgen(unchecked_param_type = "Quantity")] quantity: JsValue,
    lang: &str,
) -> Result<String, JsValue> {
    let quantity: Quantity = from_js(quantity, "quantity")?;
    Ok(format_quantity(&quantity, &LanguageTag(lang.to_owned())))
}

/// Writes a quantity back as editable text (`"1 1/2 cups"`).
///
/// # Errors
///
/// Throws if the quantity is malformed.
#[wasm_bindgen(js_name = quantityToInput)]
pub fn quantity_to_input_js(
    #[wasm_bindgen(unchecked_param_type = "Quantity")] quantity: JsValue,
    lang: &str,
) -> Result<String, JsValue> {
    let quantity: Quantity = from_js(quantity, "quantity")?;
    Ok(quantity_to_input(&quantity, &LanguageTag(lang.to_owned())))
}

/// Validates an editor draft, returning every problem with its JSON Pointer.
///
/// # Errors
///
/// Throws if the draft does not even have the payload's shape.
#[wasm_bindgen(js_name = validateDraft, unchecked_return_type = "Issue[]")]
pub fn validate_draft_js(
    #[wasm_bindgen(unchecked_param_type = "BundlePayload")] draft: JsValue,
) -> Result<JsValue, JsValue> {
    let draft: BundlePayload = from_js(draft, "draft")?;
    to_js(&validate_draft(&draft))
}

/// Creates the identity of a new recipe (fresh birth nonce).
///
/// # Errors
///
/// Throws if the system RNG fails or `now` is not whole seconds.
#[wasm_bindgen(js_name = newIdentity, unchecked_return_type = "Identity")]
pub fn new_identity_js(title: &str, lang: &str, now: f64) -> Result<JsValue, JsValue> {
    to_js(&new_identity(title, LanguageTag(lang.to_owned()), timestamp(now)?).or_js()?)
}

/// An image ready to add to a draft's `blobs`.
#[derive(Serialize, Deserialize, Tsify)]
pub struct ImageBlob {
    /// The content-addressed blob id.
    pub id: String,
    /// The blob.
    pub blob: Blob,
}

/// Wraps encoded image bytes as a content-addressed blob.
///
/// # Errors
///
/// Throws if the media type is not an allowlisted raster type or the image
/// is larger than a blob may be.
#[wasm_bindgen(js_name = imageBlob, unchecked_return_type = "ImageBlob")]
pub fn image_blob(
    bytes: &[u8],
    media_type: &str,
    width: u32,
    height: u32,
) -> Result<JsValue, JsValue> {
    let media_type = match media_type {
        "image/png" => MediaType::Png,
        "image/jpeg" => MediaType::Jpeg,
        "image/webp" => MediaType::Webp,
        "image/avif" => MediaType::Avif,
        other => {
            return Err(failure(
                &format!("{other} is not an allowed image type"),
                Some("E.BLOB.MEDIATYPE"),
                None,
            ));
        }
    };
    if bytes.len() > limits::MAX_BLOB_SIZE {
        return Err(failure(
            "the image is larger than 16 MiB",
            Some("E.LIMIT"),
            None,
        ));
    }
    let id = derive_blob_id(bytes).or_js()?;
    to_js(&ImageBlob {
        id: id.0,
        blob: Blob {
            media_type: OpenCode::Known(media_type),
            size: bytes.len() as u64,
            width: (width > 0).then_some(u64::from(width)),
            height: (height > 0).then_some(u64::from(height)),
            bytes: bytes.to_vec(),
        },
    })
}

/// Verifies a `signed-profile` and returns the profile it carries.
///
/// # Errors
///
/// Throws if the bytes are malformed or the signature is invalid.
#[wasm_bindgen(js_name = verifyProfile, unchecked_return_type = "AccountProfile")]
pub fn verify_profile_js(bytes: &[u8]) -> Result<JsValue, JsValue> {
    to_js(&crisp_core::profile::verify_profile(bytes).or_js()?)
}
