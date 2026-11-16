//! Read-only original passage adapter for native evidence verification.
use kwaai_provenance::verification::{EvidenceResolver, SourcePassage};
use kwaai_provenance::{EvidenceKind, EvidenceRef};
use kwaai_rag::meta_store::MetaStore;

/// Wraps the existing original chunk store; never uses graph retrieval results.
pub struct OriginalEvidenceResolver<'a>(pub &'a MetaStore);
impl EvidenceResolver for OriginalEvidenceResolver<'_> {
    fn resolve(&self, evidence: &EvidenceRef) -> Result<Option<SourcePassage>, String> {
        if evidence.kind != EvidenceKind::Original
            || evidence.source_id != format!("kb:{}", self.0.tenant_id())
        {
            return Ok(None);
        }
        let Some(id) = evidence.chunk_id else {
            return Ok(None);
        };
        let chunk = self
            .0
            .get_chunks(&[id])
            .map_err(|e| e.to_string())?
            .pop()
            .flatten();
        Ok(chunk.map(|c| SourcePassage {
            source_id: evidence.source_id.clone(),
            document_id: c.doc_name,
            text: c.text,
        }))
    }
}
