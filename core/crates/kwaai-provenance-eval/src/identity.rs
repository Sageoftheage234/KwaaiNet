//! Identity of the source and dependency resolution embedded in this compiled evaluator.
use crate::hash;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EvaluatorIdentity {
    pub algorithm_version: u32,
    pub sha256: String,
    pub files: BTreeMap<String, String>,
}

fn fingerprint(sources: &[(&str, &[u8])]) -> EvaluatorIdentity {
    let files: BTreeMap<String, String> = sources
        .iter()
        .map(|(name, bytes)| (name.to_string(), hash(bytes)))
        .collect();
    let mut framed = b"kwaai-provenance-eval-implementation-v1\0".to_vec();
    // BTreeMap key order and lowercase fixed-width SHA-256 make this encoding stable.
    framed.extend(serde_json::to_vec(&files).expect("string map serialization cannot fail"));
    EvaluatorIdentity {
        algorithm_version: 1,
        sha256: hash(&framed),
        files,
    }
}

/// Compile-time bytes describe the executable's implementation, not a possibly
/// edited source checkout at execution time. Keep this list in sync with modules.
pub fn implementation_identity() -> EvaluatorIdentity {
    fingerprint(&[
        (
            "core/crates/kwaai-provenance/src/lib.rs",
            include_bytes!("../../kwaai-provenance/src/lib.rs"),
        ),
        (
            "core/crates/kwaai-provenance/src/identity.rs",
            include_bytes!("../../kwaai-provenance/src/identity.rs"),
        ),
        (
            "core/crates/kwaai-provenance/src/schema.rs",
            include_bytes!("../../kwaai-provenance/src/schema.rs"),
        ),
        (
            "core/crates/kwaai-provenance/src/gate.rs",
            include_bytes!("../../kwaai-provenance/src/gate.rs"),
        ),
        (
            "core/crates/kwaai-provenance/src/verification.rs",
            include_bytes!("../../kwaai-provenance/src/verification.rs"),
        ),
        (
            "core/crates/kwaai-provenance-eval/src/phase5.rs",
            include_bytes!("phase5.rs"),
        ),
        (
            "core/crates/kwaai-provenance/src/shadow.rs",
            include_bytes!("../../kwaai-provenance/src/shadow.rs"),
        ),
        (
            "core/crates/kwaai-provenance-rag/src/shadow.rs",
            include_bytes!("../../kwaai-provenance-rag/src/shadow.rs"),
        ),
        (
            "core/crates/kwaai-provenance-eval/src/lib.rs",
            include_bytes!("lib.rs"),
        ),
        (
            "core/crates/kwaai-provenance-eval/src/main.rs",
            include_bytes!("main.rs"),
        ),
        (
            "core/crates/kwaai-provenance-eval/src/identity.rs",
            include_bytes!("identity.rs"),
        ),
        (
            "core/crates/kwaai-provenance-eval/Cargo.toml",
            include_bytes!("../Cargo.toml"),
        ),
        ("core/Cargo.toml", include_bytes!("../../../Cargo.toml")),
        ("core/Cargo.lock", include_bytes!("../../../Cargo.lock")),
        (
            "core/.cargo/config.toml",
            include_bytes!("../../../.cargo/config.toml"),
        ),
        (
            "rust-toolchain.toml",
            include_bytes!("../../../../rust-toolchain.toml"),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_deterministic_and_order_independent() {
        let a = fingerprint(&[("source", b"abc"), ("lock", b"def")]);
        let b = fingerprint(&[("lock", b"def"), ("source", b"abc")]);
        assert_eq!(a, b);
        assert_eq!(implementation_identity(), implementation_identity());
        assert_eq!(implementation_identity().files.len(), 16);
    }

    #[test]
    fn source_schema_and_lock_changes_change_identity() {
        let original = fingerprint(&[("source", b"a"), ("schema", b"1"), ("lock", b"x")]);
        for changed in [
            fingerprint(&[("source", b"b"), ("schema", b"1"), ("lock", b"x")]),
            fingerprint(&[("source", b"a"), ("schema", b"2"), ("lock", b"x")]),
            fingerprint(&[("source", b"a"), ("schema", b"1"), ("lock", b"y")]),
        ] {
            assert_ne!(original.sha256, changed.sha256);
        }
    }
}
