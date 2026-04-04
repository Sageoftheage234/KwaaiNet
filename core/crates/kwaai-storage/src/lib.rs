//! kwaai-storage — Multi-tenant vector storage fabric for KwaaiNet
//!
//! Each KwaaiNet node can act as an Eve (storage host), holding encrypted
//! vectors for multiple Bobs (data owners). This crate provides:
//!
//! - **PGVector storage** with per-tenant HNSW-indexed tables
//! - **Tenant management** (create, list, delete, capacity enforcement)
//! - **REST API** for remote Bobs to upload/search vectors
//! - **Health endpoint** compatible with KwaaiNet's DHT advertisement

pub mod api;
pub mod db;
pub mod tenant;
pub mod vectors;

pub use api::run_storage_api;
pub use db::StorageDb;
pub use tenant::{TenantInfo, TenantManager, TenantStats};
pub use vectors::{SearchResult, VectorStore};
