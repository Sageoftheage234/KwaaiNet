//! Bounded, explicitly invoked comparison of retained stage/proposal assessments.
use kwaai_provenance::verification::source_hash;
use kwaai_provenance_rag::shadow::{evaluate, summarize, ShadowInput, SCHEMA_VERSION};
use serde_json::json;
use std::{
    fs::OpenOptions,
    io::{Read, Write},
    time::Instant,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 5 {
        return Err(
            "usage: provenance-shadow INPUT.jsonl OUTPUT.jsonl LIMIT RUN_ID CORPUS_ID".into(),
        );
    }
    let limit: usize = args[2].parse()?;
    if !(1..=10_000).contains(&limit) || args[3].trim().is_empty() || args[4].trim().is_empty() {
        return Err("LIMIT must be 1..10000; RUN_ID and CORPUS_ID must be nonempty".into());
    }
    let start = Instant::now();
    let mut bytes = Vec::new();
    std::fs::File::open(&args[0])?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 16 * 1024 * 1024 {
        return Err("manifest exceeds 16 MiB".into());
    }
    let text = std::str::from_utf8(&bytes)?;
    let mut rows = Vec::new();
    for line in text.lines().take(limit) {
        let input: ShadowInput = serde_json::from_str(line)?;
        if input.context.run_id != args[3] {
            return Err("record run ID differs from RUN_ID".into());
        }
        rows.push(evaluate(input)?);
    }
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[1])?;
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
    };
    writeln!(
        output,
        "{}",
        json!({"record":"shadow_run", "schema_version":SCHEMA_VERSION,
        "timestamp":chrono::Utc::now().to_rfc3339(), "run_id":args[3],
        "git_commit":git(&["rev-parse","HEAD"]), "git_dirty":git(&["status","--porcelain"]).map(|s| !s.is_empty()),
        "manifest_sha256":source_hash(text), "limit":limit,
        "supplied_metadata":{"corpus_identity":args[4],"record_context":"caller supplied; not independently validated", "verification":"retained Phase 4 assessments; not rerun", "ground_truth":"caller assertion of independent annotation"},
        "validated_metadata":{"manifest_hash":"exact input bytes", "provenance_authentication":"none"},
        "verifier_metadata_location":"evaluation.input.assessment: result.verifier, backend, implementation; request.metadata retains available corpus hashes"})
    )?;
    for row in &rows {
        writeln!(
            output,
            "{}",
            json!({"record":"shadow_evaluation", "evaluation":row})
        )?;
    }
    let summary = json!({"record":"shadow_summary", "schema_version":SCHEMA_VERSION,"summary":summarize(&rows),"runtime_seconds":start.elapsed().as_secs_f64()});
    writeln!(output, "{summary}")?;
    output.sync_all()?;
    println!("{summary}");
    Ok(())
}
