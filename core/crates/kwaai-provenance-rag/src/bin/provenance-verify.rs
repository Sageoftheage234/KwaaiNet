//! Offline, explicitly invoked evidence evaluation. Never alters graph/observer files.
use kwaai_provenance::verification::{
    ollama::{OllamaConfig, OllamaVerifier},
    source_hash, verify, EvidenceBackend, MockVerifier, VerificationFailure, VerificationRequest,
    VERSION,
};
use kwaai_provenance::SupportStatus;
use kwaai_provenance_rag::verification::OriginalEvidenceResolver;
use kwaai_rag::meta_store::MetaStore;
use serde_json::json;
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::Path,
    time::Instant,
};
use uuid::Uuid;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if !(6..=7).contains(&args.len()) {
        return Err("usage: provenance-verify REQUESTS.jsonl DATA_DIR TENANT_UUID CORPUS_ID OUTPUT.jsonl LIMIT [OLLAMA_CONFIG.json]".into());
    }
    let limit: usize = args[5].parse()?;
    if !(1..=10_000).contains(&limit) {
        return Err("LIMIT must be 1..10000".into());
    }
    if args[3].trim().is_empty() {
        return Err("CORPUS_ID must identify the source snapshot".into());
    }
    if fs::metadata(&args[0])?.len() > 16 * 1024 * 1024 {
        return Err("manifest exceeds 16 MiB".into());
    }
    let input = fs::read(&args[0])?;
    let requests: Vec<VerificationRequest> = BufReader::new(input.as_slice())
        .lines()
        .take(limit)
        .map(|line| Ok(serde_json::from_str(&line?)?))
        .collect::<Result<_, Box<dyn std::error::Error>>>()?;
    let tenant: Uuid = args[2].parse()?;
    let meta = MetaStore::open_read_only(Path::new(&args[1]), tenant)?;
    let resolver = OriginalEvidenceResolver(&meta);
    let config: OllamaConfig = match args.get(6) {
        Some(path) => serde_json::from_slice(&fs::read(path)?)?,
        None => OllamaConfig::default(),
    };
    // Byte budget reserves space for prompt, wrappers and output. No truncation.
    let budget = config
        .num_ctx
        .saturating_sub(config.num_predict.saturating_add(2048))
        .min(24_000);
    let mut output = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&args[4])?;
    let start = Instant::now();
    let connection = OllamaVerifier::connect(config.clone()).await;
    let unavailable = MockVerifier {
        responses: BTreeMap::new(),
    };
    let (backend, startup_error): (&dyn EvidenceBackend, Option<String>) = match &connection {
        Ok(v) => (v, None),
        Err(e) => (&unavailable, Some(e.clone())),
    };
    let git = |args: &[&str]| {
        std::process::Command::new("git")
            .args(args)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
    };
    let run = json!({"record":"verification_run", "schema_version":1, "timestamp":chrono::Utc::now().to_rfc3339(), "git_commit":git(&["rev-parse","HEAD"]), "git_dirty":git(&["status","--porcelain"]).map(|s| !s.is_empty()), "implementation":VERSION,"backend":backend.configuration(),"requested_config":config,"startup_error":startup_error,"corpus_identity":args[3],"tenant":tenant,"manifest_sha256":source_hash(std::str::from_utf8(&input)?),"limit":limit,"max_input_bytes":budget});
    writeln!(output, "{run}")?;
    let (mut supported, mut unsupported, mut unresolved, mut missing, mut failures) =
        (0, 0, 0, 0, 0);
    for request in requests {
        let mut assessment = verify(&request, &resolver, backend, budget).await;
        if assessment.failure == Some(VerificationFailure::Inference) {
            if let Some(ref error) = startup_error {
                assessment.rationale = error.clone();
            }
        }
        match assessment.result.status {
            SupportStatus::Supported => supported += 1,
            SupportStatus::Unsupported => unsupported += 1,
            SupportStatus::Unresolved => unresolved += 1,
        }
        if assessment.failure == Some(VerificationFailure::MissingEvidence) {
            missing += 1;
        }
        if assessment.failure.is_some() {
            failures += 1;
        }
        writeln!(
            output,
            "{}",
            json!({"record":"evidence_verification","request":request,"assessment":assessment})
        )?;
        output.flush()?;
    }
    let summary = json!({"record":"verification_summary","claims_evaluated":supported+unsupported+unresolved,"supported":supported,"unsupported":unsupported,"unresolved":unresolved,"missing_evidence":missing,"verification_failures":failures,"runtime_seconds":start.elapsed().as_secs_f64()});
    writeln!(output, "{summary}")?;
    output.sync_all()?;
    println!("{summary}");
    Ok(())
}
