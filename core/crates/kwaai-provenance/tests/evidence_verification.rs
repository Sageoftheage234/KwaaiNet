use kwaai_provenance::{
    verification::*, Claim, ClaimKind, EvidenceKind, EvidenceRef, SupportStatus,
};
use std::collections::BTreeMap;
struct Sources(BTreeMap<String, SourcePassage>);
impl EvidenceResolver for Sources {
    fn resolve(&self, e: &EvidenceRef) -> Result<Option<SourcePassage>, String> {
        Ok(self.0.get(&e.id).cloned())
    }
}
fn fixture(texts: &[&str]) -> (VerificationRequest, Sources) {
    let evidence: Vec<_> = texts
        .iter()
        .enumerate()
        .map(|(i, t)| {
            EvidenceRef::new(
                EvidenceKind::Original,
                "doc",
                source_hash(t),
                Some(i as i64),
                vec![],
            )
        })
        .collect();
    let sources = Sources(
        evidence
            .iter()
            .zip(texts)
            .map(|(e, t)| {
                (
                    e.id.clone(),
                    SourcePassage {
                        source_id: "doc".into(),
                        document_id: "paper".into(),
                        text: t.to_string(),
                    },
                )
            })
            .collect(),
    );
    (
        VerificationRequest {
            claim: Claim::new(ClaimKind::EntityField, "Alice", "birth_year", "1901"),
            evidence,
            entity_bindings: BTreeMap::new(),
            metadata: BTreeMap::new(),
        },
        sources,
    )
}
fn backend(r: &VerificationRequest, raw: Result<&str, &str>) -> MockVerifier {
    MockVerifier {
        responses: BTreeMap::from([(
            r.claim.id.clone(),
            raw.map(str::to_owned).map_err(str::to_owned),
        )]),
    }
}
#[tokio::test]
async fn independently_labeled_cases_and_repeatability() {
    // Human specified labels exercise orchestration; the mock is not an NLI model.
    for (texts, status) in [
        (vec!["Alice was born in 1901."], SupportStatus::Supported),
        (
            vec!["Alice was born in 1902, not 1901."],
            SupportStatus::Unsupported,
        ),
        (vec!["Alice likes tea."], SupportStatus::Unresolved),
        (
            vec!["Alice was born in 1901.", "Alice was born in 1902."],
            SupportStatus::Unresolved,
        ),
    ] {
        let (r, s) = fixture(&texts);
        let raw = serde_json::json!({"status":status,"rationale":"Independent fixture verdict"})
            .to_string();
        let b = backend(&r, Ok(&raw));
        let a = verify(&r, &s, &b, 100_000).await;
        let again = verify(&r, &s, &b, 100_000).await;
        assert_eq!(a.result.status, status);
        assert!(a.failure.is_none());
        assert_eq!(
            serde_json::to_value(a).unwrap(),
            serde_json::to_value(again).unwrap()
        );
    }
}
#[tokio::test]
async fn missing_and_altered_evidence_never_reaches_supported_backend() {
    let (mut r, mut s) = fixture(&["Alice was born in 1901."]);
    let b = backend(&r, Ok(r#"{"status":"supported","rationale":"yes"}"#));
    let id = r.evidence[0].id.clone();
    s.0.get_mut(&id).unwrap().text = "tampered".into();
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.failure,
        Some(VerificationFailure::HashMismatch)
    );
    s.0.clear();
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.failure,
        Some(VerificationFailure::MissingEvidence)
    );
    r.evidence.clear();
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.result.status,
        SupportStatus::Unresolved
    );
}
#[tokio::test]
async fn malformed_responses_and_failures_are_isolated() {
    let (r, s) = fixture(&["Alice was born in 1901."]);
    for raw in [
        "garbage",
        r#"{"status":"maybe","rationale":"no"}"#,
        r#"{"status":"supported"}"#,
        r#"{"status":"supported","rationale":""}"#,
        r#"{"status":"supported","rationale":"yes","extra":true}"#,
    ] {
        let a = verify(&r, &s, &backend(&r, Ok(raw)), 100_000).await;
        assert_eq!(a.failure, Some(VerificationFailure::InvalidResponse));
        assert_eq!(a.result.status, SupportStatus::Unresolved);
    }
    let a = verify(
        &r,
        &s,
        &backend(&r, Err("timeout / model unavailable")),
        100_000,
    )
    .await;
    assert_eq!(a.failure, Some(VerificationFailure::Inference));
    let b = backend(
        &r,
        Ok(r#"{"status":"supported","rationale":"explicit birth year"}"#),
    );
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.result.status,
        SupportStatus::Supported
    );
    assert_eq!(
        verify(&r, &s, &b, 1).await.failure,
        Some(VerificationFailure::InputTooLarge)
    );
}
#[tokio::test]
async fn identities_bindings_and_derived_sources_are_checked() {
    let (mut r, s) = fixture(&["Alice was born in 1901."]);
    let b = backend(&r, Ok(r#"{"status":"supported","rationale":"yes"}"#));
    r.claim.subject = "someone else".into();
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.failure,
        Some(VerificationFailure::InvalidIdentity)
    );
    r.claim = Claim::new(ClaimKind::EntityField, "entity:1", "birth_year", "1901");
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.failure,
        Some(VerificationFailure::InvalidIdentity)
    );
    r.entity_bindings.insert("entity:1".into(), "Alice".into());
    r.evidence[0] = EvidenceRef::new(EvidenceKind::Derived, "doc", "", Some(0), vec![]);
    assert_eq!(
        verify(&r, &s, &b, 100_000).await.failure,
        Some(VerificationFailure::MissingEvidence)
    );
}

#[cfg(feature = "ollama")]
#[tokio::test]
#[ignore = "requires an installed local Ollama model; transport smoke only, not accuracy evaluation"]
async fn local_ollama_smoke() {
    let (r, s) = fixture(&["Alice was born in 1901."]);
    let backend = ollama::OllamaVerifier::connect(ollama::OllamaConfig::default())
        .await
        .unwrap();
    let result = verify(&r, &s, &backend, 5000).await;
    println!("{}", serde_json::to_string(&result).unwrap());
    assert!(result.failure.is_none(), "{}", result.rationale);
    // Deliberately no model-label assertion: this is a transport/schema smoke.
}
