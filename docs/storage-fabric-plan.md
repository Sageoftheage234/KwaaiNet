# Storage Fabric: Multi-Tenant Eve Nodes

_Audience: contributors, protocol implementers, node operators_

KwaaiNet's compute layer (block-sharded inference) is shipped. The storage fabric is the next milestone: each node hosts a **multi-tenant encrypted vector database** (the Eve role from VPK), allowing remote data owners (Bobs) to rent space and shard their knowledge bases across multiple Eve nodes on the network.

> **Related docs:**
> - [VPK vision](https://github.com/Kwaai-AI-Lab/PHE/blob/main/docs/vision.md) — privacy-preserving knowledge layer design
> - [`roadmap.md`](roadmap.md) — storage layer gaps (section 3)
> - [`network-and-intent-routing.md`](network-and-intent-routing.md) — intent lifecycle protocol
> - [Vector DB benchmarking study](https://github.com/Kwaai-AI-Lab/vector_dbs_benchmarking) — PGVector selection rationale

---

## Architecture Overview

```
  Bob (Data Owner)                    Eve (Storage Host)
  ┌──────────────┐                    ┌──────────────────────┐
  │  PHE binary   │                    │  PHE binary (Eve)    │
  │  ┌──────────┐ │    encrypted      │  ┌────────────────┐  │
  │  │ Encrypt  │ │───vectors────────▶│  │  PGVector       │  │
  │  │ Scramble │ │    HTTP API       │  │  (per-tenant    │  │
  │  │ Fan-out  │ │◀──scores─────────│  │   HNSW tables)  │  │
  │  └──────────┘ │                    │  └────────────────┘  │
  │  PostgreSQL   │                    │  PostgreSQL          │
  │  (plaintext)  │                    │  (encrypted only)    │
  └──────────────┘                    └──────────────────────┘
         │                                      │
         └──── KwaaiNet DHT (discovery) ────────┘
```

**Key properties:**
- Eve never sees plaintext — only encrypted vectors and scrambled IDs
- Bob encrypts locally, then fans out to remote Eves over HTTP
- Each Eve hosts multiple Bobs (tenants) with schema-based isolation
- Tenant discovery and capacity advertisement happens via DHT

---

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Eve vector DB backend | **PGVector** | Most memory-efficient (9.9GB), most consistent (CV=1.4%), HNSW+IVFFlat. Sweet spot 10k–500k vectors/tenant. Reuses existing PHE PostgreSQL dependency. |
| Tenant isolation | **Per-tenant PGVector tables** | `eve_vectors_{tenant_hex8}` with individual HNSW indexes. No cross-tenant FKs. Clean capacity tracking via `pg_total_relation_size()`. |
| PHE integration model | **Separate binary** | PHE stays in its own repo. KwaaiNet manages PG lifecycle and discovers/advertises PHE via DHT. Same pattern as p2pd. |
| PG provisioning | **KwaaiNet-managed** | `kwaainet storage init` installs PG+pgvector, creates a dedicated data dir, runs migrations. Operators don't need to be DBAs. |
| Auth (Phase 1) | **PeerId + tenant_secret** | Bob registers with PeerId, Eve issues a UUID API key. Simple, functional. Designed for upgrade to Ed25519-signed requests via intent protocol. |
| HNSW tuning | **m=16, ef_construction=64** | Optimized for 10k–500k chunks per tenant per benchmarking results. |

---

## Phase 0: Eve Operator CLI — Storage Setup

**Repo: KwaaiNet** | **New command: `kwaainet storage init`**

An Eve operator runs one command to provision their node for storage rental.

```
kwaainet storage init [OPTIONS]
  --capacity-gb <GB>       Max storage to offer (default: 5)
  --data-path <PATH>       Where to store PG data (default: ~/.kwaainet/storage)
  --port <PORT>            VPK API port (default: 7432)
  --endpoint <URL>         Public endpoint to advertise on DHT (optional)
```

**What it does:**

1. Checks/installs PostgreSQL + pgvector (Homebrew on macOS, apt on Linux)
2. Initializes a dedicated PG data directory at `~/.kwaainet/storage/pgdata` (avoids conflicts with system PG, uses port 5433)
3. Starts PG, creates `kwaainet_vpk` database, enables pgvector extension
4. Downloads PHE binary (same pattern as `kwaainet setup --get-deps` for p2pd)
5. Runs PHE database migrations
6. Saves config to `~/.kwaainet/config.yaml`:
   ```yaml
   vpk_enabled: true
   vpk_mode: "eve"
   vpk_local_port: 7432
   storage:
     pg_url: "postgresql://localhost:5433/kwaainet_vpk"
     data_path: "~/.kwaainet/storage"
     capacity_gb: 5
     pg_port: 5433
   ```

**Related commands:**
- `kwaainet storage status` — PG health, disk usage, tenant count, capacity remaining
- `kwaainet storage start/stop` — manage PG lifecycle
- `kwaainet storage destroy` — remove everything (with confirmation)

**PG lifecycle wired into `kwaainet start/stop`:** when storage config exists, PG and PHE are started/stopped alongside p2pd and the node.

**New files:** `storage.rs` (handler), `StorageConfig` struct in `config.rs`, `Storage(StorageArgs)` in `cli.rs`

---

## Phase 1: Database Migration for Multi-Tenancy

**Repo: PHE** | **New file: `migrations/20260404_004_multi_tenancy.sql`**

Add multi-tenant schema to PHE's PostgreSQL:

- **`tenants` table** — `tenant_id` (UUID PK), `peer_id` (Bob's PeerId base58), `display_name`, `capacity_limit_mb`, `status` (Active/Suspended/Deleted), timestamps
- **`tenant_id` column** added to `documents`, `index_mapping`, `audit_log`, `encryption_keys` (nullable for backward compat)
- **Default tenant** auto-created on upgrade; existing NULL-tenant rows adopted

**Files:** `src/database/mod.rs` (migration + bootstrap), `src/database/queries.rs` (new `TenantQueries`)

---

## Phase 2: PGVector Eve Storage Backend

**Repo: PHE** | **New file: `src/vectordb/pgvector.rs`**

Replace in-memory brute-force with PGVector. Extract a `VectorStorage` trait so both backends coexist.

**`VectorStorage` trait:**
```rust
async fn upload_vectors(&self, vectors: Vec<(i64, Array1<f64>)>) -> VPKResult<usize>;
async fn search(&self, query: &Array1<f64>, top_k: usize) -> VPKResult<Vec<SearchResult>>;
async fn delete_vectors(&self, indices: Vec<i64>) -> VPKResult<usize>;
async fn health_check(&self) -> VPKResult<HealthStatus>;
async fn clear(&self) -> VPKResult<()>;
fn count(&self) -> usize;
```

**`PgVectorClient`** creates per-tenant tables with HNSW index (m=16, ef_construction=64). Existing `VectorDBClient` renamed to `InMemoryVectorStorage` (used in tests).

**`ShardManager`** updated to use `Arc<dyn VectorStorage>` instead of `Arc<VectorDBClient>`.

**New dep:** `pgvector = "0.4"`, `EveStorageConfig` in `config.rs`

---

## Phase 3: Tenant-Aware VPK Core

**Repo: PHE** | **Most invasive change**

Thread `tenant_id` through the VPK orchestrator.

- **`TenantManager`** — CRUD + capacity enforcement (`src/tenant/manager.rs`)
- **`TenantContext`** — per-tenant crypto state (keys, scrambler, pipeline, shard_manager), lazy-loaded on first request
- All VPK methods gain `tenant_id` parameter: `upload_document`, `upload_documents`, `query`
- Per-tenant index mapping caches
- Existing parameterless methods route to "default" tenant for backward compat

---

## Phase 4: Eve HTTP API for Remote Bobs

**Repo: PHE** | **New routes in `src/api/`**

Eve exposes a tenant-aware REST API that remote Bobs call:

| Route | Method | Purpose |
|-------|--------|---------|
| `/api/tenants` | POST | Create tenant (Bob registers on Eve) |
| `/api/tenants` | GET | List tenants |
| `/api/tenants/{id}` | GET | Tenant info + stats |
| `/api/tenants/{id}` | DELETE | Soft-delete tenant |
| `/api/tenants/{id}/vectors` | POST | Upload encrypted vectors |
| `/api/tenants/{id}/search` | POST | Search encrypted vectors |
| `/api/tenants/{id}/vectors` | DELETE | Delete vectors |
| `/api/health` | GET | Global health + tenant_count + capacity |

All vector endpoints accept **already-encrypted** data. Eve never sees plaintext.

Enhanced `/api/health` response includes `tenant_count`, `capacity_gb_available`, `total_vectors`, `version`, `peer_id` — compatible with what KwaaiNet's `node.rs` already polls.

Existing `/api/upload`, `/api/search` continue to work via default tenant.

---

## Phase 5: Remote Shard Client

**Repo: PHE** | **New file: `src/vectordb/remote.rs`**

Bob's `ShardManager` fans out to **remote Eve HTTP endpoints** instead of local in-memory shards.

**`RemoteEveClient`** implements `VectorStorage` over HTTP:
- `upload_vectors()` → `POST /api/tenants/{id}/vectors`
- `search()` → `POST /api/tenants/{id}/search`
- `delete_vectors()` → `DELETE /api/tenants/{id}/vectors`

**Bob's workflow:**
1. `kwaainet vpk discover` — find Eve nodes on DHT
2. Create tenants on chosen Eves (`POST /api/tenants`)
3. Configure PHE `shard_configs` with Eve URLs + tenant IDs
4. Upload: encrypt locally → fan out to remote Eves
5. Query: encrypt query → fan out → merge results → decrypt

---

## Phase 6: KwaaiNet CLI Updates

**Repo: KwaaiNet**

**Tenant management commands:**
```
kwaainet vpk tenant create --peer-id <ID> --capacity-mb 1024 [--eve-endpoint <URL>]
kwaainet vpk tenant list [--eve-endpoint <URL>]
kwaainet vpk tenant info <TENANT_ID> [--eve-endpoint <URL>]
kwaainet vpk tenant delete <TENANT_ID> [--eve-endpoint <URL>]
```

When `--eve-endpoint` is omitted, targets local VPK. When provided, targets a remote Eve.

**Enhanced `kwaainet vpk status`** — per-tenant breakdown from health endpoint.

**DHT update** — add `total_vectors: u64` to `VpkInfo` struct in `node.rs`.

---

## Auth Model: Storage Intent Protocol

Per [`network-and-intent-routing.md`](network-and-intent-routing.md), storage rental follows the same 6-phase intent lifecycle as inference:

| Phase | Storage equivalent |
|-------|-------------------|
| Intent | Bob broadcasts "I need N MB encrypted vector storage, trust >= Verified" |
| Discovery | Eve nodes query DHT for matching storage intents |
| Offer | Eve responds with capacity, SLAs, credentials |
| Contract | Tenant creation = contract execution |
| Fulfillment | Bob uploads encrypted vectors; Eve stores and serves queries |
| Settlement | Usage reporting, reputation updates |

**This plan implements simplified Phase 1 auth:**
- Bob creates tenant with PeerId; Eve issues `tenant_secret` (UUID API key)
- No signature verification; trust is implicit (Eve is on DHT, Bob chose it)
- All endpoints accept optional `Authorization` header (ignored now, enforced later)
- `peer_id` recorded everywhere for future identity binding
- Audit log captures all operations for future settlement

**Phase 2 auth (future):** Ed25519-signed requests, local trust score checks, IntentOffer/Contract formalization, usage metering, reputation updates.

---

## Implementation Order

```
Phase 0 (KwaaiNet: storage CLI)  ── independent, can start immediately
Phase 1 (PHE: DB migration)      ─┐
Phase 2 (PHE: PgVector backend)   ├── can be parallel
                                   │
Phase 3 (PHE: Tenant core)        ┘── depends on 1+2
Phase 4 (PHE: Eve HTTP API)       ── depends on 3
Phase 5 (PHE: Remote client)      ── depends on 4
Phase 6 (KwaaiNet: CLI updates)   ── depends on 4, can parallel with 5
```

---

## Verification

**Unit tests (PHE):** PgVectorClient ops, TenantManager CRUD, RemoteEveClient with mock server, InMemoryVectorStorage backward compat.

**Integration test (both repos):**
```bash
# Eve node
kwaainet storage init --capacity-gb 10
kwaainet start --daemon

# Bob creates tenant on Eve
kwaainet vpk tenant create --peer-id $(kwaainet identity show) \
  --capacity-mb 1024 --eve-endpoint http://eve-host:7432

# Bob uploads and queries via remote Eve
# Cross-tenant isolation: tenant A data invisible to tenant B

# Health
kwaainet vpk status        # tenant count, capacity
kwaainet vpk discover      # Eve metrics in DHT
```

**Backward compat:** existing single-tenant PHE upgrades seamlessly (default tenant auto-created, existing data adopted).
