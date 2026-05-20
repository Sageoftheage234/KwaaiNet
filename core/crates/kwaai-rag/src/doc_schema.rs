use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SectionDef {
    /// Case-insensitive substring matched against paragraph text to detect this section heading.
    pub pattern: String,
    /// When true, chunks in this section are skipped during graph/entity extraction.
    #[serde(default)]
    pub skip: bool,
    /// Injected into the extraction prompt and dream evidence when this section is active.
    pub narrator_note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DocSchema {
    /// The exact title of the source document — used to prevent treating it as a location entity.
    pub document_title: Option<String>,
    /// Name of the default narrator / first-person voice in the main body.
    pub default_narrator: Option<String>,
    /// Ordered list of section definitions; first pattern to match wins.
    #[serde(default)]
    pub sections: Vec<SectionDef>,
    /// Free-form document-level facts persisted into the KB and injected into every query.
    /// Well-known keys: "author", "subject", "year", "language", "publisher".
    #[serde(default)]
    pub metadata: std::collections::HashMap<String, String>,
}

impl DocSchema {
    /// Build a human-readable one-line context string from the metadata map, suitable for
    /// prepending to LLM prompts so the model always knows who wrote the document.
    pub fn context_line(&self) -> Option<String> {
        if self.metadata.is_empty() && self.document_title.is_none() {
            return None;
        }
        let mut parts: Vec<String> = Vec::new();
        let title = self
            .document_title
            .as_deref()
            .or_else(|| self.metadata.get("title").map(|s| s.as_str()))
            .unwrap_or("(untitled document)");
        if let Some(author) = self.metadata.get("author") {
            parts.push(format!("\"{}\" by {}", title, author));
        } else {
            parts.push(format!("\"{}\"", title));
        }
        if let Some(year) = self.metadata.get("year") {
            parts.push(format!("({})", year));
        }
        if let Some(subject) = self.metadata.get("subject") {
            parts.push(format!("Subject: {}", subject));
        }
        Some(parts.join(" "))
    }
}

pub fn load_doc_schema(path: &Path) -> Result<DocSchema> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading doc schema: {}", path.display()))?;
    let schema: DocSchema =
        serde_yaml::from_str(&text).with_context(|| "parsing doc schema YAML")?;
    Ok(schema)
}

/// Returns the first `SectionDef` whose pattern is a case-insensitive substring of `heading`.
pub fn match_section<'a>(heading: &str, schema: &'a DocSchema) -> Option<&'a SectionDef> {
    let lower = heading.to_lowercase();
    schema
        .sections
        .iter()
        .find(|s| lower.contains(&s.pattern.to_lowercase()))
}
