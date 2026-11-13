//! Observe-only provenance for the `kwaai-rag` dream cycle.
//!
//! Implements [`kwaai_rag::dream::DreamObserver`] with `kwaai-provenance`: after
//! each dream stage it compares the graph's facts with the previous stage and
//! records every difference as a lineage event (extraction, evidence added,
//! rewrite, removal), runs the provenance gate in Observe mode on facts the cycle
//! introduced, and writes a per-cycle summary with the traceability rate.
//!
//! It never changes the dream cycle: the hooks receive the graph read-only, any
//! error or panic stops observation without reaching the cycle, and the gate is
//! never in Enforce mode here.
//!
//! Turned on only when the `kwaainet` CLI is built with `--features provenance`
//! and [`ENV_DIR`] names a directory. See [`observer_from_env`].

#![warn(missing_docs)]

pub mod facts;
pub mod observer;
pub mod plan;

pub use observer::{
    CycleSummary, OriginCounts, ProvenanceObserver, StageCounts, ENV_DIR, LINEAGE_FILE,
};

use kwaai_rag::dream::{DreamObserver, NoopObserver};
use std::path::PathBuf;
use uuid::Uuid;

/// Observer selected by the environment, plus a one-line status for the CLI to
/// print. With [`ENV_DIR`] unset or empty this is a no-op observer and no
/// status. If the directory cannot be used the status says why, and the observer
/// is a no-op: a dream cycle never fails because provenance could not start.
pub fn observer_from_env(tenant: Uuid) -> (Box<dyn DreamObserver + Send>, Option<String>) {
    let dir = match std::env::var_os(ENV_DIR) {
        Some(d) if !d.is_empty() => PathBuf::from(d),
        _ => return (Box::new(NoopObserver), None),
    };
    match ProvenanceObserver::open(&dir, tenant) {
        Ok(obs) => {
            let cycle = obs.cycle().unwrap_or(0);
            (
                Box::new(obs),
                Some(format!(
                    "observing cycle {cycle} → {} (observe-only)",
                    dir.display()
                )),
            )
        }
        Err(e) => (Box::new(NoopObserver), Some(format!("NOT recording: {e}"))),
    }
}
