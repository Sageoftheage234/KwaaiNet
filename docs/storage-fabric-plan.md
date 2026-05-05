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
  Bob (Data Owner)                         Eve (Storage Host)
  ┌───────────────────┐                    ┌──────────────────────────┐
  │  PHE library      │                    │  kwaainet (Eve role)     │
  │  ┌─────────────┐  │  /kwaai/storage/   │  ┌──────────────────┐   │
  │  │ Encrypt     │  │   1.0.0 (P2P,      │  │  kwaai-storage   │   │
  │  │ Scramble    │──┼──Noise, PeerId) ──▶│  │  redb + HNSW     │   │
  │  │ Fan-out     │  │◀──indices+scores───│  │  (per-tenant     │   │
  │  └─────────────┘  │                    │  │   indices)       │   │
  │                   │                    │  └──────────────────┘   │
  └───────────────────┘                    └──────────────────────────┘
         │                                              │
         └──────── KwaaiNet DHT (PeerId discovery) ─────┘
```

**Key properties:**
- Eve never sees plaintext — only encrypted vectors and scrambled IDs
- Bob encrypts locally, then fans out to remote Eves **via P2P relay (no HTTP, no open ports)**
- Nodes are addressed by PeerId only — IP addresses are never advertised or needed
- Each Eve hosts multiple Bobs (tenants) with isolated HNSW indices
- Tenant discovery and capacity advertisement happens via DHT

---

## Key Decisions

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Eve vector DB backend | **redb + hnsw_rs (embedded)** | Zero external dependencies — no PostgreSQL or Docker required. redb for durable vector persistence, hnsw_rs for in-memory ANN search rebuilt on startup. Ships as part of the kwaainet binary. |
| Tenant isolation | **Per-tenant in-memory HNSW index** | Each tenant gets a `TenantIndex` with its own HNSW graph, id_map, and tombstone layer. Keys in redb are `tenant_id(16) ++ doc_id(8)` — no cross-tenant FKs. |
| Remote access protocol | **`/kwaai/storage/1.0.0` (P2P relay)** | Follows the same model as `/kwaai/inference/1.0.0`. Noise-encrypted, PeerId-authenticated, NAT-transparent via libp2p circuit relay. No open ports required. |
| HTTP API scope | **`127.0.0.1:7432` only (operator console)** | Local `kwaainet storage status` and dashboards. Never exposed to remote peers. Remote access always goes through the P2P relay. |
| Node addressing | **PeerId only** | DHT records contain `peer_id`, `mode`, `capacity_gb`, `tenant_count`, `vpk_version`. No IP addresses or HTTP endpoints. Volatile IPs are irrelevant — libp2p resolves routing by PeerId. |
| PHE integration model | **Embedded library (PHE repo)** | PHE encryption algorithms are applied locally by Bob before calling `rpc_upload_vectors`. Eve stores and searches opaque float arrays. |
| Auth (Phase 1) | **PeerId recorded on CreateTenant** | Bob's PeerId is the authoritative identity on tenant creation. No separate API key. Designed for upgrade to Ed25519-signed request envelopes in Phase 2. |
| HNSW tuning | **m=16, ef_construction=64** | Optimized for 10k–500k chunks per tenant per benchmarking results. |
| Index strategy | **Flat first, HNSW optional** | See "Index Strategy" section below. When Bob shards across many Eves, each holds a small slice where brute-force beats HNSW. |

---

## Phase 0: Eve Operator CLI — Storage Setup

**Repo: KwaaiNet** | **New command: `kwaainet storage init`**

An Eve operator runs one command to provision their node for storage rental.

```
kwaainet storage init [OPTIONS]
  --capacity-gb <GB>       Max storage to offer (default: 5)
  --data-dir <PATH>        Where to store vector data (default: ~/.kwaainet/storage)
  --port <PORT>            Local health-check API port (default: 7432; localhost only)
```

**What it does:**

1. Opens (or creates) the embedded redb store at `~/.kwaainet/storage/metadata.redb`
2. Saves config to `~/.kwaainet/config.yaml`:
   ```yaml
   vpk_enabled: true
   vpk_mode: "eve"
   vpk_local_port: 7432
   storage:
     data_dir: "~/.kwaainet/storage"
     capacity_gb: 5
   ```

No PostgreSQL, Docker, or port forwarding required. Remote Bobs connect via the P2P relay using the node's PeerId.

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

## Phase 4: P2P Storage Protocol (already implemented)

**Repo: KwaaiNet** | **File: `core/crates/kwaai-cli/src/storage_rpc.rs`**

Remote Bobs access Eve storage via `/kwaai/storage/1.0.0` — a libp2p unary RPC protocol. This is the canonical remote access path: all inter-node storage communication uses this protocol. **No HTTP port forwarding is required.** The libp2p relay (circuit relay v2) handles NAT traversal automatically.

**Wire format** (msgpack over libp2p stream):

Request: `{ op: StorageOp, tenant_id: Option<String>, payload: Vec<u8> }`  
Response: `{ ok: bool, payload: Vec<u8>, error: Option<String> }`

**Operations** (`StorageOp` enum):

| Operation | Input payload | Output payload |
|-----------|---------------|----------------|
| `Health` | — | `HealthPayload { status, tenant_count, total_vectors, capacity_gb_total, capacity_gb_available, version, peer_id }` |
| `CreateTenant` | `CreateTenantPayload { peer_id, capacity_limit_mb, display_name, vector_dimension }` | `TenantInfo` |
| `GetTenant` | — (tenant_id in envelope) | `TenantInfo` |
| `ListTenants` | — | `Vec<TenantInfo>` |
| `DeleteTenant` | — (tenant_id in envelope) | — |
| `UploadVectors` | `UploadPayload { vectors: Vec<{ id, embedding }> }` | `usize` (count uploaded) |
| `SearchVectors` | `SearchPayload { query: Vec<f32>, top_k }` | `Vec<SearchResult { id, score }>` |
| `DeleteVectors` | `DeleteVectorsPayload { ids: Vec<i64> }` | `usize` (count deleted) |

**Authentication (Phase 1):** Bob's `PeerId` is recorded on `CreateTenant`. Phase 2 will add Ed25519-signed request envelopes.

**Local operator HTTP API** (`127.0.0.1:7432`, localhost only): the same operations are also available as a REST API for `kwaainet storage status` and operator dashboards. This interface is never exposed remotely.

**Handler registration:** when `kwaainet storage serve` starts, it registers `make_storage_rpc_handler()` with the running p2pd via the IPC socket. If p2pd is not running, the P2P relay is unavailable (HTTP-only mode for local use).

---

## Phase 5: Bob Fan-Out Client

**Repo: KwaaiNet** | **File: `core/crates/kwaai-cli/src/storage_rpc.rs`** (client helpers already present)

> **⚠️ Sharding is a capacity mechanism, not a latency optimisation.**
>
> Empirical benchmarks (May 2026, K=2 metro Eves and K=11 geographically diverse Eves) show
> WAN fan-out query latency is dominated entirely by P2P round-trip time:
>
> | | K=2 (metro) | K=11 (diverse) |
> |-|-------------|----------------|
> | P2P RTT p50 | 25.6 ms | 92.5 ms |
> | HNSW compute/shard | 1–1.3 ms | ~0.2 ms |
> | Sharded search p50 | 32 ms | 114 ms |
> | Local HNSW p50 | 2.2–2.5 ms | 2.0–2.6 ms |
> | WAN breakeven | K ≈ 2⁶³ shards | ~9.8B vectors |
>
> Fan-out latency equals **max(shard RTTs)**, not the mean. Adding fast Eves to a fleet that
> contains any slow Eve does not help — the slowest node sets the floor for every query.
>
> **The correct use case for sharding is RAM capacity:** when Bob's knowledge base is too
> large to fit in a single Eve's available memory. For latency-sensitive workloads, Bob
> should store on a single nearby Eve (or locally). See `docs/vpk-shard-bench/README.md`
> for full empirical data.

Bob's fan-out to remote Eves uses the P2P client helpers in `storage_rpc.rs`:

```rust
// Discover Eves via DHT
let eves = kwaainet vpk discover --json   // returns [{peer_id, mode, capacity_gb, ...}]

// Create tenant on chosen Eve (by PeerId, no IP needed)
let tenant = rpc_create_tenant(&p2p_client, &eve_peer_id, CreateTenantPayload { ... }).await?;

// Apply PHE encryption locally — Eve receives opaque vectors
let encrypted = phe_encrypt(&vectors, &tenant_key);

// Fan out to each Eve shard (parallel uploads)
rpc_upload_vectors(&p2p_client, &eve_peer_id, tenant.id, encrypted).await?;

// Query: encrypt → fan out in parallel → merge → decrypt
let results = rpc_search_vectors(&p2p_client, &eve_peer_id, tenant.id, query_vec, top_k).await?;
```

The PHE encryption library is applied locally by Bob before any network call. Eve stores and searches only opaque float arrays — it has no knowledge of the encryption algorithm, the model used, or the plaintext content.

**Bob's workflow:**
1. `kwaainet vpk discover` — find Eve nodes by PeerId via DHT
2. `rpc_create_tenant()` — register as a tenant on each chosen Eve (authenticated by Bob's PeerId)
3. Encrypt vectors locally using PHE library
4. `rpc_upload_vectors()` — fan out to each Eve shard **in parallel** (uploads and queries both)
5. Query: encrypt query locally → `rpc_search_vectors()` parallel fan out → merge results → decrypt → lookup local docs

**When to use multiple Eves:** only when the corpus exceeds a single Eve's RAM. Querying a
single nearby Eve is always faster on WAN than fan-out across multiple Eves.

---

## Phase 6: KwaaiNet CLI Updates

**Repo: KwaaiNet**

**Tenant management commands** (to be implemented — `VpkAction::Tenant` subcommand):
```
kwaainet vpk tenant create --eve-peer-id <PeerId> --capacity-mb 1024
kwaainet vpk tenant list   --eve-peer-id <PeerId>
kwaainet vpk tenant info   <TENANT_ID> --eve-peer-id <PeerId>
kwaainet vpk tenant delete <TENANT_ID> --eve-peer-id <PeerId>
```

`--eve-peer-id` is the Eve node's base58 PeerId (obtained from `kwaainet vpk discover`). No IP address or HTTP endpoint is ever required. When `--eve-peer-id` is omitted, commands target the local node.

Under the hood, all tenant commands call the corresponding `rpc_*` helper in `storage_rpc.rs` via the running p2pd daemon socket.

**Enhanced `kwaainet vpk status`** — per-tenant breakdown from health endpoint.

**DHT update** — add `total_vectors: u64` to `VpkInfo` struct in `node.rs`.

---

## Index Strategy: Flat vs HNSW

> **HNSW vs flat search is a compute and memory optimisation — not a user-visible latency decision on WAN.**
> Empirical bench (May 2026, K=2 and K=11 WAN Eves) shows P2P RTT dominates total query time at 25–93 ms.
> HNSW compute savings are at most ~1.5 ms per query — imperceptible at any realistic WAN round-trip.

For single-Eve local access, HNSW delivers 1–2.5 ms p50 search (empirically measured). For small corpora
(< 10K vectors per shard), brute-force flat scan matches HNSW quality while avoiding graph-maintenance overhead.
The flat-first approach reduces per-shard memory use and simplifies correctness at small scale.

**In a sharded deployment, each Eve holds a small slice.** A Bob with a 1GB knowledge base (~250k vectors
at 384-dim) sharded across 100 Eves means each Eve holds ~2,500 vectors. At that scale:

- Brute-force flat search (sequential scan) is O(n) but with tiny n
- HNSW index adds write overhead (graph maintenance) with no read benefit below ~10K vectors
- Flat search uses less memory (no graph structure)

**Plan: default to flat (no index), with HNSW as a per-tenant opt-in when vector count exceeds a threshold.**

The `ensure_table()` method should:
1. Create the table without an HNSW index (flat search)
2. When vector count crosses a configurable threshold (e.g., 10k), automatically create the HNSW index
3. Expose the threshold as a config option so operators can tune it

Note: the HNSW/flat choice has no measurable impact on WAN query latency. It matters only when Bob queries a
single Eve over a low-latency (LAN or loopback) connection.

> **Current implementation**: HNSW index is always created. This will be changed to flat-first with auto-HNSW
> after benchmarks validate the exact crossover point on Eve-class hardware.

---

## Security: Embedding Model as Bob's Secret

**Eve never embeds.** The choice of embedding model — and any fine-tuning or customization — is Bob's function and part of his security posture. If Eve knew both the embedding model and the stored vectors, she could reconstruct semantic meaning from the embeddings, undermining the privacy model.

Bob's embedding pipeline:
1. Bob selects an embedding model (e.g., all-MiniLM-L6-v2, a fine-tuned domain model, or a custom projection)
2. Bob embeds his documents locally, producing float vectors
3. Bob sends only the opaque float vectors to Eve (optionally encrypted via PHE)
4. Bob embeds his queries locally using the same model before sending to Eve

Eve sees only arrays of floats. She computes `cosine(query_vec, stored_vec)` and returns `(id, score)`. She has no knowledge of:
- Which embedding model was used
- What the dimensions represent semantically
- What the original text was

This also means different Bobs on the same Eve can use entirely different embedding models and dimensions. Eve is model-agnostic — she just does math on whatever vectors Bob gives her.

---

## Security: Search by Index Protocol

A key security property of the storage fabric: **Eve never returns vectors, only indices and scores.**

When Bob searches, Eve computes cosine similarity on stored vectors and returns only:
- **Scrambled document IDs** (opaque integers — not real document IDs)
- **Similarity scores**

Eve never returns the stored vectors themselves. Bob then:
1. Reverse-maps scrambled IDs → real document IDs using his **private index mapping table** (a secret held only by Bob)
2. Looks up the actual documents in his **local knowledge base**

This provides three security benefits:
- **Bandwidth reduction**: returning indices + scores is 100-1000x less data than returning full vectors
- **No vector leakage**: even if Eve is compromised, encrypted vectors never cross the network boundary during search
- **Index mapping as a secret**: the scrambled_id ↔ real_doc_id mapping is known only to Bob, providing an additional layer of privacy beyond encryption

The current API already returns only `{id, score}` pairs (no vectors). When encryption is added later, the `id` field becomes a scrambled ID that only Bob can decode, and vectors stored on Eve are encrypted so even Eve's database administrator cannot read them.

> **Not yet implemented**: Index scrambling and mapping are part of the PHE encryption layer. The current plaintext implementation uses real document IDs. When PHE is integrated, Bob will scramble IDs before upload and unscramble after search.

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

**Integration test:**
```bash
# Eve node (no port forwarding needed)
kwaainet storage init --capacity-gb 10
kwaainet start --daemon

# Verify DHT record has no IP/endpoint
kwaainet vpk discover --json | jq '.[0] | keys'
# → ["capacity_gb", "mode", "peer_id", "tenant_count", "vpk_version"]

# Remote Bob — connect by PeerId only
EVE_PEER_ID=$(kwaainet vpk discover --json | jq -r '.[0].peer_id')
kwaainet vpk tenant create --eve-peer-id $EVE_PEER_ID --capacity-mb 1024

# Verify P2P health check reaches Eve
# (relay status shown in kwaainet vpk discover output)

# Verify local HTTP is not reachable remotely
curl http://localhost:7432/api/health          # ✅ works (Eve's own machine)
curl http://<eve-public-ip>:7432/api/health    # ❌ should be unreachable (bound to 127.0.0.1)

# Health
kwaainet vpk status        # local health via port 7432
kwaainet vpk discover      # Eve PeerId + metrics in DHT
```

**Backward compat:** existing single-tenant PHE upgrades seamlessly (default tenant auto-created, existing data adopted).
