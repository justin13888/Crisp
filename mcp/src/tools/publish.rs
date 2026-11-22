//! `publish_recipe`: upload, check what the server returned, and keep the
//! endorsed copy only if nothing newer was saved meanwhile.

use tracing::{info, warn};

use super::ops::{Authoring, identify};
use super::verdict::verdict;
use super::views::PublishOutput;

impl Authoring {
    /// `publish_recipe`.
    ///
    /// # Errors
    ///
    /// Returns a message when the recipe is not in the library or the server
    /// refuses it.
    pub async fn publish(&self, doc_id: &str) -> Result<PublishOutput, String> {
        let server = self.gateway.url().ok_or_else(|| {
            "no Crisp server is configured: set CRISP_SERVER_URL and CRISP_SERVER_USERNAME"
                .to_owned()
        })?;
        let bytes = self
            .library
            .read(doc_id)
            .map_err(|e| e.0)?
            .ok_or_else(|| format!("no recipe {doc_id} in the library; save it first"))?;
        let (_, uploaded) = identify(&bytes).map_err(|e| format!("{doc_id} does not open: {e}"))?;
        let endorsed = self.gateway.publish(bytes).await.map_err(|e| e.0)?;
        let (returned_doc, returned) = identify(&endorsed)
            .map_err(|e| format!("the server returned an unreadable file: {e}"))?;
        if returned_doc != doc_id || returned != uploaded {
            return Err(format!(
                "the server returned a different document or revision ({returned_doc} {returned}); \
                 the library copy is unchanged"
            ));
        }
        let subject = format!("the endorsed copy of {doc_id}");
        let (endorsed, verdict) = verdict(&subject, endorsed, self.clock.now()).await?;
        if !verdict.passes() {
            let reasons = verdict.reasons();
            warn!(doc_id, %reasons, "endorsed copy refused");
            return Err(format!(
                "the server returned a file that does not verify ({reasons}); the library copy \
                 is unchanged"
            ));
        }
        // Keep the endorsed copy only if no save replaced the uploaded
        // revision while the upload was in flight.
        let _writing = self.writes.lock().await;
        let current = self.library.read(doc_id).map_err(|e| e.0)?;
        let still_current = current
            .as_deref()
            .map(identify)
            .transpose()?
            .is_some_and(|(_, revision)| revision == uploaded);
        if still_current {
            self.library.write(doc_id, &endorsed).map_err(|e| e.0)?;
        } else {
            warn!(
                doc_id,
                "a newer revision was saved during publishing; kept it"
            );
        }
        info!(doc_id, %server, revision = %uploaded, "recipe published");
        Ok(PublishOutput {
            doc_id: doc_id.to_owned(),
            revision: uploaded,
            server,
            stored_endorsed: still_current,
        })
    }
}
