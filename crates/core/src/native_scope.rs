//! Read-only comparison of observed logical IDs and a native spawn subtree.
//! An aligned set is evidence only: it never authorizes a real deletion.
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
pub const VERIFIED_NATIVE_VERSION: &str = "0.162.1";
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NativeScopeEvidence {
    pub runtime_version: String,
    pub root_id: String,
    pub observed_ids: Vec<String>,
    pub native_ids: Vec<String>,
    pub observed_complete: bool,
    pub native_complete: bool,
}
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NativeScopeAssessment {
    pub aligned: bool,
    pub missing_from_native: Vec<String>,
    pub unexpected_native_ids: Vec<String>,
    pub blocked_reasons: Vec<String>,
    pub execution_enabled: bool,
}
fn uuid(value: &str) -> Option<String> {
    let bytes = value.as_bytes();
    if bytes.len() != 36
        || !bytes.iter().enumerate().all(|(i, b)| {
            if [8, 13, 18, 23].contains(&i) {
                *b == b'-'
            } else {
                b.is_ascii_hexdigit()
            }
        })
    {
        return None;
    }
    Some(value.to_ascii_lowercase())
}
fn identifiers(values: &[String]) -> Option<BTreeSet<String>> {
    if values.is_empty() || values.len() > 20_000 {
        return None;
    }
    let normalized: Option<BTreeSet<_>> = values.iter().map(|v| uuid(v)).collect();
    normalized.filter(|set| set.len() == values.len())
}
pub fn assess(evidence: &NativeScopeEvidence) -> NativeScopeAssessment {
    let mut reasons = Vec::new();
    if evidence.runtime_version != VERIFIED_NATIVE_VERSION {
        reasons.push("nativeVersionUnsupported".into());
    }
    if !evidence.observed_complete || !evidence.native_complete {
        reasons.push("nativeScopeIncomplete".into());
    }
    let observed = identifiers(&evidence.observed_ids);
    let native = identifiers(&evidence.native_ids);
    let root = uuid(&evidence.root_id);
    let (missing, unexpected) =
        if let (Some(observed), Some(native), Some(root)) = (observed, native, root) {
            if !observed.contains(&root) || !native.contains(&root) {
                reasons.push("nativeRootMissing".into());
            }
            let missing = observed.difference(&native).cloned().collect::<Vec<_>>();
            let unexpected = native.difference(&observed).cloned().collect::<Vec<_>>();
            if !missing.is_empty() || !unexpected.is_empty() {
                reasons.push("nativeScopeMismatch".into());
            }
            (missing, unexpected)
        } else {
            reasons.push("nativeIdsInvalid".into());
            (vec![], vec![])
        };
    NativeScopeAssessment {
        aligned: reasons.is_empty(),
        missing_from_native: missing,
        unexpected_native_ids: unexpected,
        blocked_reasons: reasons,
        execution_enabled: false,
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    const ROOT: &str = "00000000-0000-4000-8000-000000000001";
    const CHILD: &str = "00000000-0000-4000-8000-000000000002";
    const OTHER: &str = "00000000-0000-4000-8000-000000000003";
    fn evidence() -> NativeScopeEvidence {
        NativeScopeEvidence {
            runtime_version: VERIFIED_NATIVE_VERSION.into(),
            root_id: ROOT.into(),
            observed_ids: vec![ROOT.into(), CHILD.into()],
            native_ids: vec![CHILD.into(), ROOT.into()],
            observed_complete: true,
            native_complete: true,
        }
    }
    #[test]
    fn equal_sets_are_evidence_not_permission() {
        let p = assess(&evidence());
        assert!(p.aligned);
        assert!(!p.execution_enabled);
        assert!(crate::cleanup::reject_live_execution().is_err());
    }
    #[test]
    fn equal_counts_do_not_hide_different_ids() {
        let mut e = evidence();
        e.native_ids[0] = OTHER.into();
        let p = assess(&e);
        assert!(!p.aligned);
        assert_eq!(p.missing_from_native, vec![CHILD]);
        assert_eq!(p.unexpected_native_ids, vec![OTHER]);
    }
    #[test]
    fn missing_root_duplicate_invalid_and_empty_sets_are_rejected() {
        for mode in 0..4 {
            let mut e = evidence();
            match mode {
                0 => e.root_id = OTHER.into(),
                1 => e.native_ids = vec![ROOT.into(), ROOT.into()],
                2 => e.observed_ids = vec!["../outside".into()],
                _ => e.native_ids.clear(),
            };
            assert!(!assess(&e).aligned);
        }
    }
    #[test]
    fn unsupported_versions_and_incomplete_projections_are_rejected() {
        for mode in 0..3 {
            let mut e = evidence();
            match mode {
                0 => e.runtime_version = "0.154.0".into(),
                1 => e.native_complete = false,
                _ => e.observed_complete = false,
            };
            assert!(!assess(&e).aligned);
        }
    }
    #[test]
    fn canonical_uuid_case_is_normalized_but_duplicates_are_not_laundered() {
        let mut e = evidence();
        let id = "abcdef00-0000-4000-8000-000000000001";
        e.root_id = id.into();
        e.observed_ids = vec![id.to_uppercase()];
        e.native_ids = vec![id.into()];
        assert!(assess(&e).aligned);
        e.native_ids.push(id.to_uppercase());
        assert!(!assess(&e).aligned);
    }
}
