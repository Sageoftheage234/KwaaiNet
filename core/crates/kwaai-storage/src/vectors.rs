//! Per-tenant vector storage backed by PGVector.
//!
//! Each tenant gets a dedicated table (`eve_vectors_{tenant_hex8}`) with
//! an HNSW index for fast cosine similarity search.

use anyhow::{Context, Result};
use pgvector::Vector;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::StorageDb;
use crate::tenant::vector_table_name;

/// A single search result: scrambled ID + similarity score.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub id: i64,
    pub score: f64,
}

/// Per-tenant PGVector storage.
#[derive(Clone)]
pub struct VectorStore {
    db: StorageDb,
}

impl VectorStore {
    pub fn new(db: StorageDb) -> Self {
        Self { db }
    }

    /// Ensure the per-tenant vector table and HNSW index exist.
    pub async fn ensure_table(&self, tenant_id: Uuid, dimension: usize) -> Result<()> {
        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);

        // Create table + HNSW index (idempotent)
        client
            .batch_execute(&format!(
                r#"
CREATE TABLE IF NOT EXISTS {table} (
    id          BIGINT PRIMARY KEY,
    embedding   VECTOR({dimension}) NOT NULL,
    created_at  TIMESTAMPTZ DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_{table}_hnsw
    ON {table} USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
"#,
            ))
            .await
            .with_context(|| format!("creating vector table {}", table))?;

        Ok(())
    }

    /// Upload a batch of vectors for a tenant.
    ///
    /// Each vector is a `(id, embedding)` pair. IDs are opaque to Eve —
    /// they may be scrambled document IDs from Bob's encryption pipeline,
    /// or plain document IDs in the no-encryption case.
    pub async fn upload(
        &self,
        tenant_id: Uuid,
        vectors: &[(i64, Vec<f32>)],
    ) -> Result<usize> {
        if vectors.is_empty() {
            return Ok(0);
        }

        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);

        // Prepared statement for upsert — executed once per vector.
        // For large batches a COPY approach would be faster, but this is
        // simpler and sufficient for typical upload sizes (100s of vectors).
        let stmt = client
            .prepare(&format!(
                "INSERT INTO {} (id, embedding) VALUES ($1, $2) \
                 ON CONFLICT (id) DO UPDATE SET embedding = EXCLUDED.embedding, created_at = NOW()",
                table
            ))
            .await
            .context("prepare upload statement")?;

        let mut uploaded = 0usize;
        for (id, embedding) in vectors {
            let vec = Vector::from(embedding.clone());
            client.execute(&stmt, &[id, &vec]).await.context("insert vector")?;
            uploaded += 1;
        }

        Ok(uploaded)
    }

    /// Search for the top-K most similar vectors using cosine distance.
    pub async fn search(
        &self,
        tenant_id: Uuid,
        query: &[f32],
        top_k: usize,
    ) -> Result<Vec<SearchResult>> {
        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);
        let query_vec = Vector::from(query.to_vec());

        let rows = client
            .query(
                &format!(
                    "SELECT id, 1 - (embedding <=> $1) AS score FROM {} ORDER BY embedding <=> $1 LIMIT $2",
                    table
                ),
                &[&query_vec, &(top_k as i64)],
            )
            .await
            .context("vector search")?;

        Ok(rows
            .iter()
            .map(|r| SearchResult {
                id: r.get(0),
                score: r.get(1),
            })
            .collect())
    }

    /// Delete vectors by IDs.
    pub async fn delete(
        &self,
        tenant_id: Uuid,
        ids: &[i64],
    ) -> Result<usize> {
        if ids.is_empty() {
            return Ok(0);
        }

        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);

        let n = client
            .execute(
                &format!("DELETE FROM {} WHERE id = ANY($1)", table),
                &[&ids],
            )
            .await
            .context("delete vectors")?;

        Ok(n as usize)
    }

    /// Count vectors for a tenant.
    pub async fn count(&self, tenant_id: Uuid) -> Result<i64> {
        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);

        // If table doesn't exist yet, return 0
        let exists = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
                &[&table],
            )
            .await?;
        if !exists.get::<_, bool>(0) {
            return Ok(0);
        }

        let row = client
            .query_one(&format!("SELECT COUNT(*) FROM {}", table), &[])
            .await?;
        Ok(row.get(0))
    }
}
