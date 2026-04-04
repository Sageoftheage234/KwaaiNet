//! Tenant management — CRUD and capacity enforcement.

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::db::StorageDb;

/// Public tenant info returned by the API.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantInfo {
    pub tenant_id: Uuid,
    pub peer_id: String,
    pub display_name: Option<String>,
    pub capacity_limit_mb: i64,
    pub status: String,
    pub created_at: String,
}

/// Per-tenant storage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TenantStats {
    pub tenant_id: Uuid,
    pub vector_count: i64,
    pub storage_bytes: i64,
}

/// Tenant lifecycle manager.
#[derive(Clone)]
pub struct TenantManager {
    db: StorageDb,
}

impl TenantManager {
    pub fn new(db: StorageDb) -> Self {
        Self { db }
    }

    /// Create a new tenant for a given Bob (peer_id).
    pub async fn create(
        &self,
        peer_id: &str,
        capacity_limit_mb: i64,
        display_name: Option<&str>,
    ) -> Result<TenantInfo> {
        let client = self.db.client().await?;
        let row = client
            .query_one(
                r#"INSERT INTO tenants (peer_id, display_name, capacity_limit_mb)
                   VALUES ($1, $2, $3)
                   RETURNING tenant_id, peer_id, display_name, capacity_limit_mb, status,
                             created_at::text"#,
                &[&peer_id, &display_name, &capacity_limit_mb],
            )
            .await
            .context("create tenant")?;

        Ok(TenantInfo {
            tenant_id: row.get(0),
            peer_id: row.get(1),
            display_name: row.get(2),
            capacity_limit_mb: row.get(3),
            status: row.get(4),
            created_at: row.get(5),
        })
    }

    /// List all active tenants.
    pub async fn list(&self) -> Result<Vec<TenantInfo>> {
        let client = self.db.client().await?;
        let rows = client
            .query(
                r#"SELECT tenant_id, peer_id, display_name, capacity_limit_mb, status,
                          created_at::text
                   FROM tenants
                   WHERE status != 'Deleted'
                   ORDER BY created_at"#,
                &[],
            )
            .await
            .context("list tenants")?;

        Ok(rows
            .iter()
            .map(|r| TenantInfo {
                tenant_id: r.get(0),
                peer_id: r.get(1),
                display_name: r.get(2),
                capacity_limit_mb: r.get(3),
                status: r.get(4),
                created_at: r.get(5),
            })
            .collect())
    }

    /// Get a single tenant by ID.
    pub async fn get(&self, tenant_id: Uuid) -> Result<Option<TenantInfo>> {
        let client = self.db.client().await?;
        let row = client
            .query_opt(
                r#"SELECT tenant_id, peer_id, display_name, capacity_limit_mb, status,
                          created_at::text
                   FROM tenants
                   WHERE tenant_id = $1 AND status != 'Deleted'"#,
                &[&tenant_id],
            )
            .await
            .context("get tenant")?;

        Ok(row.map(|r| TenantInfo {
            tenant_id: r.get(0),
            peer_id: r.get(1),
            display_name: r.get(2),
            capacity_limit_mb: r.get(3),
            status: r.get(4),
            created_at: r.get(5),
        }))
    }

    /// Soft-delete a tenant and drop its vector table.
    pub async fn delete(&self, tenant_id: Uuid) -> Result<()> {
        let client = self.db.client().await?;

        // Soft-delete the tenant row
        let n = client
            .execute(
                "UPDATE tenants SET status = 'Deleted', updated_at = NOW() WHERE tenant_id = $1",
                &[&tenant_id],
            )
            .await
            .context("delete tenant")?;

        if n == 0 {
            bail!("tenant not found: {}", tenant_id);
        }

        // Drop the vector table (frees disk space)
        let table = vector_table_name(tenant_id);
        client
            .batch_execute(&format!("DROP TABLE IF EXISTS {}", table))
            .await
            .context("drop vector table")?;

        Ok(())
    }

    /// Get storage stats for a tenant.
    pub async fn stats(&self, tenant_id: Uuid) -> Result<TenantStats> {
        let client = self.db.client().await?;
        let table = vector_table_name(tenant_id);

        // Check if vector table exists
        let exists = client
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM information_schema.tables WHERE table_name = $1)",
                &[&table],
            )
            .await?;
        let table_exists: bool = exists.get(0);

        if !table_exists {
            return Ok(TenantStats {
                tenant_id,
                vector_count: 0,
                storage_bytes: 0,
            });
        }

        let count_row = client
            .query_one(&format!("SELECT COUNT(*) FROM {}", table), &[])
            .await?;
        let vector_count: i64 = count_row.get(0);

        let size_row = client
            .query_one(
                &format!("SELECT pg_total_relation_size('{}')", table),
                &[],
            )
            .await?;
        let storage_bytes: i64 = size_row.get(0);

        Ok(TenantStats {
            tenant_id,
            vector_count,
            storage_bytes,
        })
    }

    /// Count active tenants.
    pub async fn count(&self) -> Result<i64> {
        let client = self.db.client().await?;
        let row = client
            .query_one(
                "SELECT COUNT(*) FROM tenants WHERE status = 'Active'",
                &[],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Total vectors across all tenants.
    pub async fn total_vectors(&self) -> Result<i64> {
        let client = self.db.client().await?;
        let rows = client
            .query(
                "SELECT table_name FROM information_schema.tables WHERE table_name LIKE 'eve_vectors_%'",
                &[],
            )
            .await?;

        let mut total: i64 = 0;
        for row in &rows {
            let table: &str = row.get(0);
            let count_row = client
                .query_one(&format!("SELECT COUNT(*) FROM {}", table), &[])
                .await?;
            total += count_row.get::<_, i64>(0);
        }
        Ok(total)
    }
}

/// Deterministic table name for a tenant's vectors.
pub fn vector_table_name(tenant_id: Uuid) -> String {
    // Use first 8 hex chars of UUID for readability
    let hex = tenant_id.as_simple().to_string();
    format!("eve_vectors_{}", &hex[..8])
}
