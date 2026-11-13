//! Print the traceability of every observed dream cycle in a provenance directory.
//!
//! ```text
//! cargo run -p kwaai-provenance-rag --bin provenance-report -- <dir> [--csv]
//! ```
//!
//! Replays `lineage.jsonl` with full validation first, and checks that the most
//! recent cycle summary's digest matches the replayed log, so a table is only
//! printed for a history that is intact.

use kwaai_provenance::persist::{self, TailPolicy};
use kwaai_provenance_rag::{CycleSummary, LINEAGE_FILE};
use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut dir: Option<PathBuf> = None;
    let mut csv = false;
    for a in args.by_ref() {
        match a.as_str() {
            "--csv" => csv = true,
            "-h" | "--help" => {
                println!("usage: provenance-report <dir> [--csv]");
                return ExitCode::SUCCESS;
            }
            _ => dir = Some(PathBuf::from(a)),
        }
    }
    let Some(dir) = dir else {
        eprintln!("usage: provenance-report <dir> [--csv]");
        return ExitCode::from(2);
    };
    match run(&dir, csv) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("provenance-report: {e}");
            ExitCode::FAILURE
        }
    }
}

fn run(dir: &std::path::Path, csv: bool) -> Result<(), String> {
    let lineage = dir.join(LINEAGE_FILE);
    let read = persist::read_file(&lineage, TailPolicy::Strict)
        .map_err(|e| format!("{}: {e}", lineage.display()))?;
    let log = read
        .replay()
        .map_err(|e| format!("{}: {e}", lineage.display()))?;
    let digest = persist::lineage_digest(&log).map_err(|e| e.to_string())?;

    let mut summaries: Vec<CycleSummary> = Vec::new();
    let mut entries: Vec<_> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .flatten()
        .filter(|e| {
            let n = e.file_name().to_string_lossy().to_string();
            n.starts_with("cycle-") && n.ends_with(".json")
        })
        .collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let bytes =
            std::fs::read(e.path()).map_err(|err| format!("{}: {err}", e.path().display()))?;
        let s: CycleSummary = serde_json::from_slice(&bytes)
            .map_err(|err| format!("{}: {err}", e.path().display()))?;
        summaries.push(s);
    }
    if summaries.is_empty() {
        return Err(format!("no cycle-*.json summaries in {}", dir.display()));
    }
    let last = summaries.last().expect("non-empty");
    let intact = last.lineage_digest == digest && last.lineage_entries == log.entries().len();

    if csv {
        println!("cycle,end,complete,facts,relation_facts,field_facts,traceable,traceability_rate,extracted,evidence_added,rewritten,removed,drift,lineage_conflicts,gate_checked,gate_would_block");
    } else {
        println!(
            "{:>5}  {:<26} {:>7} {:>9} {:>8}  {:>7} {:>8} {:>7} {:>6}  {:>10}",
            "cycle",
            "end",
            "facts",
            "traceable",
            "rate",
            "added",
            "rewrote",
            "removed",
            "drift",
            "would-block"
        );
    }
    for s in &summaries {
        let mut dream = kwaai_provenance_rag::StageCounts::default();
        let mut drift = 0usize;
        for (actor, c) in &s.changes {
            if actor == "untracked" {
                drift += c.extracted + c.evidence_added + c.rewritten + c.removed;
            } else if actor.starts_with("dream.") {
                dream.extracted += c.extracted;
                dream.evidence_added += c.evidence_added;
                dream.rewritten += c.rewritten;
                dream.removed += c.removed;
            }
        }
        let rate = s
            .traceability_rate
            .map(|r| format!("{:.1}%", r * 100.0))
            .unwrap_or_else(|| "-".into());
        if csv {
            println!(
                "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}",
                s.cycle,
                s.end,
                s.complete,
                s.facts,
                s.relation_facts,
                s.field_facts,
                s.traceable,
                s.traceability_rate
                    .map(|r| r.to_string())
                    .unwrap_or_default(),
                dream.extracted,
                dream.evidence_added,
                dream.rewritten,
                dream.removed,
                drift,
                s.lineage_conflicts,
                s.gate_checked,
                s.gate_would_block
            );
        } else {
            println!(
                "{:>5}  {:<26} {:>7} {:>9} {:>8}  {:>7} {:>8} {:>7} {:>6}  {:>10}",
                s.cycle,
                if s.complete {
                    s.end.clone()
                } else {
                    format!("{} (INCOMPLETE)", s.end)
                },
                s.facts,
                s.traceable,
                rate,
                dream.extracted,
                dream.rewritten,
                dream.removed,
                drift,
                s.gate_would_block
            );
        }
    }
    if !csv {
        println!();
        println!(
            "lineage: {} entries, digest {}… — {}",
            log.entries().len(),
            &digest[..12],
            if intact {
                "matches the latest summary"
            } else {
                "DOES NOT match the latest summary"
            }
        );
        if summaries.iter().any(|s| !s.complete) {
            println!(
                "note: INCOMPLETE cycles stopped observing part-way; see their `error` field."
            );
        }
        println!("note: traceable means lineage reaches a source chunk, not that the chunk supports the fact.");
    }
    if intact {
        Ok(())
    } else {
        Err("the lineage file changed after the latest summary was written".into())
    }
}
