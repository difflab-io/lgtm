//! The same source and placement cases run in the Neovim implementation.
use lgtm_backend::models::{Anchor, AnchorEvidence, AnchorSource};
use serde::Deserialize;
use serde_json::Value;

// fixtures --------------------------------------------------------------------
#[derive(Deserialize)]
struct Case {
    name: String,
    anchor: Anchor,
    evidence: Option<AnchorEvidence>,
    path: String,
    displayed_source: Option<AnchorSource>,
    content: Option<String>,
    path_confirmed: bool,
    expected: Value,
}

// tests -----------------------------------------------------------------------
#[test]
fn should_resolve_shared_source_and_placement_cases_without_changing_original_evidence() {
    let cases: Vec<Case> =
        serde_json::from_str(include_str!("../../../e2e/common/anchor_cases.json")).unwrap();
    for case in cases {
        let original = serde_json::to_value((&case.anchor, &case.evidence)).unwrap();
        let actual = serde_json::to_value(case.anchor.get_placement(
            case.evidence.as_ref(),
            &case.path,
            case.displayed_source.as_ref(),
            case.content.as_deref(),
            case.path_confirmed,
        ))
        .unwrap();
        assert_eq!(
            actual["status"], case.expected["status"],
            "{}: {actual}",
            case.name
        );
        for field in ["path", "start", "end", "kind"] {
            if let Some(expected) = case.expected.get(field) {
                assert_eq!(
                    &actual["location"][field], expected,
                    "{}: {actual}",
                    case.name
                );
            }
        }
        assert_eq!(
            serde_json::to_value((&case.anchor, &case.evidence)).unwrap(),
            original,
            "{} mutated its original",
            case.name
        );
    }
}
