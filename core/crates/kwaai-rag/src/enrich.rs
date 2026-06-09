//! Entity description enrichment: build paragraph summaries from all evidence chunks.
//!
//! `enrich_entity_descriptions()` iterates entities, collects all linked chunk text,
//! calls an LLM to write a concise description, and persists + re-embeds the result.
//! Think of it as a targeted dream pass focused solely on summary quality.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use anyhow::{Context, Result};
use tokio::sync::{mpsc, Semaphore};
use uuid::Uuid;

use crate::embedder::EmbedClient;
use crate::graph::GraphStore;
use crate::meta_store::MetaStore;

// ── Config ────────────────────────────────────────────────────────────────────

pub struct EnrichConfig {
    /// Only enrich entities of these types (default: Person, Place, Organization).
    pub entity_types: Vec<String>,
    /// Skip entities with fewer mentions than this (default: 2).
    pub min_mentions: u32,
    /// Maximum entities to enrich in one run (default: all).
    pub limit: usize,
    /// Parallel LLM workers (default: 4).
    pub workers: usize,
    /// Max chunks to include as evidence per entity (default: 20).
    pub fetch_limit: usize,
}

impl Default for EnrichConfig {
    fn default() -> Self {
        Self {
            entity_types: vec![
                "Person".into(),
                "Place".into(),
                "Organization".into(),
            ],
            min_mentions: 2,
            limit: usize::MAX,
            workers: 4,
            fetch_limit: 20,
        }
    }
}

#[derive(Debug, Default)]
pub struct EnrichReport {
    pub entities_processed: usize,
    pub entities_updated: usize,
    pub entities_skipped_no_evidence: usize,
    pub errors: Vec<String>,
}

// ── Work item ─────────────────────────────────────────────────────────────────

struct WorkItem {
    id: i64,
    name: String,
    entity_type: String,
    current_desc: String,
    evidence_text: String,
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Enrich entity descriptions for all qualifying entities in the knowledge base.
///
/// Progress callback receives `(done, total, label)`.
pub async fn enrich_entity_descriptions(
    cfg: &EnrichConfig,
    model: &str,
    inference_url: &str,
    embed: &EmbedClient,
    data_dir: &Path,
    tenant_id: Uuid,
    progress: impl Fn(usize, usize, &str) + Send + Sync,
) -> Result<EnrichReport> {
    let mut report = EnrichReport::default();

    // ── Phase 1: build work items (evidence text loaded here) ─────────────────
    let work_items: Vec<WorkItem> = {
        let store = GraphStore::open(data_dir, tenant_id)?;
        let meta = MetaStore::open(data_dir, tenant_id)?;

        let mut items: Vec<WorkItem> = Vec::new();

        for node in store.all_entities() {
            if !cfg
                .entity_types
                .iter()
                .any(|t| t.eq_ignore_ascii_case(&node.entity_type))
            {
                continue;
            }
            if node.mention_count < cfg.min_mentions {
                continue;
            }

            let chunk_ids = store.chunks_for_entity(node.id).to_vec();
            if chunk_ids.is_empty() {
                report.entities_skipped_no_evidence += 1;
                continue;
            }

            let fetch_limit = chunk_ids.len().min(cfg.fetch_limit);
            let chunks = match meta.get_chunks(&chunk_ids[..fetch_limit]) {
                Ok(c) => c,
                Err(_) => {
                    report.entities_skipped_no_evidence += 1;
                    continue;
                }
            };

            let evidence_text: String = chunks
                .iter()
                .flatten()
                .map(|c| {
                    let mut s = String::new();
                    if let Some(ref sec) = c.section_name {
                        s.push_str(&format!("[Section: {sec}]\n"));
                    }
                    s.push_str(&c.text);
                    s
                })
                .collect::<Vec<_>>()
                .join("\n---\n");

            if evidence_text.is_empty() {
                report.entities_skipped_no_evidence += 1;
                continue;
            }

            items.push(WorkItem {
                id: node.id,
                name: node.name.clone(),
                entity_type: node.entity_type.clone(),
                current_desc: node.description.clone(),
                evidence_text,
            });

            if items.len() >= cfg.limit {
                break;
            }
        }
        items
    }; // store + meta dropped here

    let total = work_items.len();
    if total == 0 {
        return Ok(report);
    }

    // ── Phase 2: fan-out LLM calls ────────────────────────────────────────────
    let sem = Arc::new(Semaphore::new(cfg.workers.max(1)));
    let url_arc = Arc::new(inference_url.to_string());
    let model_arc = Arc::new(model.to_string());
    let counter = Arc::new(AtomicUsize::new(0));

    let (tx, mut rx) = mpsc::channel::<(i64, Option<String>)>(total.max(1));

    for item in work_items {
        let permit = sem.clone().acquire_owned().await.expect("semaphore closed");
        let tx = tx.clone();
        let url = url_arc.clone();
        let model_clone = model_arc.clone();
        let counter = counter.clone();

        tokio::spawn(async move {
            let _permit = permit;
            let desc =
                call_summarize(&item.name, &item.entity_type, &item.current_desc, &item.evidence_text, &url, &model_clone)
                    .await;
            counter.fetch_add(1, Ordering::Relaxed);
            let _ = tx.send((item.id, desc)).await;
        });
    }
    drop(tx);

    let mut updates: Vec<(i64, String)> = Vec::new();
    let mut done = 0usize;
    while let Some((eid, desc_opt)) = rx.recv().await {
        done += 1;
        progress(done, total, "enriching");
        if let Some(desc) = desc_opt {
            updates.push((eid, desc));
        }
    }

    report.entities_processed = done;

    // ── Phase 3: write updates + re-embed ─────────────────────────────────────
    let mut store = GraphStore::open(data_dir, tenant_id)
        .context("open graph for writes")?;

    for (eid, new_desc) in &updates {
        let node = match store.get_entity(*eid).cloned() {
            Some(n) => n,
            None => continue,
        };
        let embed_text =
            GraphStore::entity_embed_text(&node.name, &node.aliases, new_desc);
        match embed.embed_batch(&[embed_text.as_str()]).await {
            Ok(embs) if !embs.is_empty() => {
                let updated = crate::graph::EntityNode {
                    description: new_desc.clone(),
                    embedding: embs.into_iter().next().unwrap(),
                    ..node
                };
                if let Err(e) = store.upsert_entity(updated) {
                    report.errors.push(format!("upsert entity {eid}: {e}"));
                } else {
                    report.entities_updated += 1;
                }
            }
            _ => {
                report.errors.push(format!("embed failed for entity {eid}"));
            }
        }
    }

    Ok(report)
}

// ── LLM summarization call ────────────────────────────────────────────────────

async fn call_summarize(
    name: &str,
    entity_type: &str,
    current_desc: &str,
    evidence_text: &str,
    url: &str,
    model: &str,
) -> Option<String> {
    let type_label = match entity_type.to_lowercase().as_str() {
        "person" => "person",
        "place" | "location" => "place or location",
        "organization" => "organization or group",
        _ => "entity",
    };

    let existing_hint = if !current_desc.is_empty() {
        format!("\nExisting summary: {current_desc}\n")
    } else {
        String::new()
    };

    let prompt = format!(
        "You are a knowledge extraction assistant working with a historical memoir.{existing_hint}\n\
         Below are all excerpts from the document that mention \"{name}\" (a {type_label}):\n\n\
         {evidence_text}\n\n\
         Based ONLY on the excerpts above, write a concise 2–3 sentence description of \"{name}\" \
         that captures: who or what they are, their significance in the story, and any key \
         relationships or roles. Do NOT add information not present in the excerpts. \
         Output ONLY the description paragraph."
    );

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(120))
        .connect_timeout(std::time::Duration::from_secs(10))
        .build()
        .ok()?;

    let full_url = format!("{}/v1/chat/completions", url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": prompt}],
        "temperature": 0.2,
        "max_tokens": 300,
    });

    let resp = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        client.post(&full_url).json(&body).send(),
    )
    .await
    .ok()?
    .ok()?;

    if !resp.status().is_success() {
        return None;
    }

    let v: serde_json::Value = tokio::time::timeout(
        std::time::Duration::from_secs(120),
        resp.json(),
    )
    .await
    .ok()?
    .ok()?;

    let text = v["choices"][0]["message"]["content"]
        .as_str()
        .unwrap_or("")
        .trim()
        .to_string();

    if text.len() < 20 {
        None
    } else {
        Some(text)
    }
}
