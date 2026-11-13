//! Read-only observation hooks for the dream cycle.
//!
//! [`run_dream_cycle_observed`](crate::dream::run_dream_cycle_observed) calls a
//! [`DreamObserver`] at the start of a cycle, after each stage that can change
//! the graph, and at the end. Every method receives the graph (and, at the start,
//! the chunk store) by shared reference only, so an observer cannot change what
//! the cycle does: it can read state, not write it. Methods return nothing, so an
//! observer cannot change control flow either.
//!
//! [`run_dream_cycle`](crate::dream::run_dream_cycle) uses [`NoopObserver`],
//! whose methods are empty, so existing callers behave exactly as before.
//!
//! This module has no dependencies beyond `kwaai-rag` itself. Provenance
//! recording is implemented outside this crate, in `kwaai-provenance-rag`.

use crate::graph::GraphStore;
use crate::meta_store::MetaStore;

/// A stage of the dream cycle after which the graph may have changed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DreamStage<'a> {
    /// Step 5: type, field, description and relation completions were written.
    Completion,
    /// Step 6: near-duplicate entities were merged, alias into canonical.
    Merge {
        /// The `(alias_id, canonical_id)` pairs the cycle attempted, in order.
        /// A pair whose merge failed is still listed; compare graph state to see
        /// what actually changed.
        pairs: &'a [(i64, i64)],
    },
    /// Step 7: zombie entities were deleted.
    Prune,
    /// Step 8: the relation integrity pass ran.
    Sanitize,
}

/// How a cycle ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CycleEnd {
    /// All stages ran.
    Completed,
    /// No entity needed work; nothing was written.
    NothingToDo,
    /// Completions all failed, so the destructive stages were skipped.
    SkippedDestructiveStages,
}

/// Read-only callbacks around a dream cycle. All methods default to doing
/// nothing; implement only the ones you need.
///
/// Implementations must not panic and must not block for long: they run inline
/// on the cycle's task. An implementation that hits an error should record it
/// and stop observing, not abort the cycle.
pub trait DreamObserver {
    /// The cycle has opened the graph and chunk store and is about to score.
    fn cycle_started(&mut self, _graph: &GraphStore, _meta: &MetaStore) {}

    /// A stage that can change the graph has finished.
    fn stage_finished(&mut self, _stage: DreamStage<'_>, _graph: &GraphStore) {}

    /// The cycle is about to return its report.
    fn cycle_finished(&mut self, _graph: &GraphStore, _end: CycleEnd) {}
}

/// Observer that does nothing. Used by [`run_dream_cycle`](crate::dream::run_dream_cycle).
#[derive(Debug, Default, Clone, Copy)]
pub struct NoopObserver;

impl DreamObserver for NoopObserver {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dream::{run_dream_cycle, run_dream_cycle_observed, DreamConfig};
    use crate::embedder::EmbedClient;
    use uuid::Uuid;

    #[derive(Default)]
    struct Recording {
        started: usize,
        stages: Vec<String>,
        ends: Vec<CycleEnd>,
    }

    impl DreamObserver for Recording {
        fn cycle_started(&mut self, _graph: &GraphStore, _meta: &MetaStore) {
            self.started += 1;
        }
        fn stage_finished(&mut self, stage: DreamStage<'_>, _graph: &GraphStore) {
            self.stages.push(format!("{stage:?}"));
        }
        fn cycle_finished(&mut self, _graph: &GraphStore, end: CycleEnd) {
            self.ends.push(end);
        }
    }

    #[tokio::test]
    async fn empty_kb_reports_start_and_nothing_to_do_and_matches_unobserved_run() {
        // No entities → no work items → the cycle returns before any LLM call,
        // so this runs offline. The embed URL is never contacted.
        let embed = EmbedClient::new(Some("http://127.0.0.1:9".into()), None);
        let cfg = DreamConfig::default();
        let progress = None::<fn(usize, usize, &str)>;

        let observed_dir = tempfile::tempdir().unwrap();
        let mut rec = Recording::default();
        let observed = run_dream_cycle_observed(
            observed_dir.path(),
            Uuid::new_v4(),
            &embed,
            &cfg,
            &[],
            "unused",
            progress,
            &mut rec,
        )
        .await
        .unwrap();
        assert_eq!(rec.started, 1);
        assert!(rec.stages.is_empty());
        assert_eq!(rec.ends, vec![CycleEnd::NothingToDo]);

        let plain_dir = tempfile::tempdir().unwrap();
        let plain = run_dream_cycle(
            plain_dir.path(),
            Uuid::new_v4(),
            &embed,
            &cfg,
            &[],
            "unused",
            progress,
        )
        .await
        .unwrap();
        assert_eq!(observed.score_before, plain.score_before);
        assert_eq!(observed.score_after, plain.score_after);
        assert_eq!(observed.entities_merged, plain.entities_merged);
        assert_eq!(observed.entities_pruned, plain.entities_pruned);
        assert_eq!(observed.cycle_errors, plain.cycle_errors);
    }

    #[test]
    fn noop_observer_is_a_dream_observer() {
        fn takes(_: &mut (dyn DreamObserver + Send)) {}
        takes(&mut NoopObserver);
    }
}
