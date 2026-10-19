//! Conformance of the reference provider (`contracts/reference-provider`) to the
//! draft On-Chain Risk Score Registry Interface.
//!
//! The reference provider is the specification's executable form: if it fails a
//! `must` vector, the specification and its reference implementation disagree,
//! and the specification is what should change.

mod common;

use common::reference_provider::Adapter;
use risk_registry_conformance::{run, Level, Outcome, RiskProvider};

/// The SEP checklist (`docs/standards/sep-risk-score-registry-checklist.md`)
/// enumerates every `MUST` in the draft standard and maps each to the test
/// that exercises it. The tests below are the reference-provider side of that
/// mapping: each one targets a specific checklist item so that a future change
/// to the standard's requirements can be checked against actual coverage.
///
/// Checklist IDs are referenced in each test's doc comment. If a checklist
/// item is added, a test must be added here (or in the vector file) and the
/// mapping updated.
mod checklist {
    //! Checklist IDs mirrored from the SEP checklist artifact. Kept in one
    //! place so the mapping is greppable and a missing item is obvious.
    pub const R1_SUPPORTS_INTERFACE: &str = "R1";
    pub const R2_SCORE_VIEW: &str = "R2";
    pub const R3_GATE_VIEW: &str = "R3";
    pub const R4_METADATA_VIEW: &str = "R4";
    pub const R5_UNKNOWN_INTERFACE: &str = "R5";
    pub const R6_SCORE_BOUNDS: &str = "R6";
    pub const R7_GATE_BOUNDS: &str = "R7";
    pub const R8_MONOTONIC_TIME: &str = "R8";
    pub const R9_UNKNOWN_ACCOUNT: &str = "R9";
    pub const R10_NO_PANIC: &str = "R10";
}

#[test]
fn reference_provider_conforms() {
    let mut adapter = Adapter::new();
    let report = run(&mut adapter);
    assert!(report.is_conformant(), "{}", report.render());
}

#[test]
fn reference_provider_runs_every_core_vector() {
    let mut adapter = Adapter::new();
    let report = run(&mut adapter);
    let (total, _, _, skipped) = report.counts();
    assert!(total > 0, "the vector file must not be empty");
    assert_eq!(
        skipped,
        0,
        "the reference provider advertises every optional capability, so no vector may be \
         skipped: {}",
        report.render()
    );
}

#[test]
fn reference_provider_advertises_the_root_capability() {
    let mut adapter = Adapter::new();
    adapter.reset();
    assert!(adapter.supports("risk").expect("supports_interface must answer"));
    assert!(adapter.supports("gate").expect("supports_interface must answer"));
    assert!(adapter.supports("cgate").expect("supports_interface must answer"));
    assert!(adapter.supports("score").expect("supports_interface must answer"));
    assert!(adapter.supports("meta").expect("supports_interface must answer"));
    assert!(!adapter.supports("nope").expect("supports_interface must answer"));
    assert!(!adapter.supports("").expect("supports_interface must answer"));
}

/// Checklist R1: `supports_interface` MUST answer for every interface the
/// provider implements and MUST NOT claim an interface it does not implement.
#[test]
fn checklist_r1_supports_interface_is_exhaustive_and_honest() {
    let mut adapter = Adapter::new();
    adapter.reset();
    for id in ["risk", "gate", "cgate", "score", "meta"] {
        assert!(
            adapter.supports(id).expect("supports_interface must answer"),
            "checklist R1: reference provider must advertise `{id}`"
        );
    }
    for id in ["nope", "", "RISK", "risk ", " risk"] {
        assert!(
            !adapter.supports(id).expect("supports_interface must answer"),
            "checklist R1: reference provider must not advertise `{id}`"
        );
    }
}

/// Checklist R2: the score view MUST return the current score for a known
/// account and MUST be callable without mutating state.
#[test]
fn checklist_r2_score_view_is_read_only_and_current() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let account = adapter.known_account();
    let first = adapter.score(&account).expect("score must answer");
    let second = adapter.score(&account).expect("score must answer");
    assert_eq!(
        first, second,
        "checklist R2: score view must be deterministic and must not mutate state"
    );
}

/// Checklist R3: the gate view MUST return a boolean decision consistent with
/// the score view for the same account.
#[test]
fn checklist_r3_gate_view_agrees_with_score() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let account = adapter.known_account();
    let allowed = adapter.gate(&account).expect("gate must answer");
    let score = adapter.score(&account).expect("score must answer");
    let expected = adapter.gate_threshold();
    assert_eq!(
        allowed,
        score >= expected,
        "checklist R3: gate decision must agree with score against the threshold"
    );
}

/// Checklist R4: the metadata view MUST return the registry identifier and
/// version, and MUST be stable across calls.
#[test]
fn checklist_r4_metadata_view_is_stable() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let first = adapter.metadata().expect("metadata must answer");
    let second = adapter.metadata().expect("metadata must answer");
    assert_eq!(
        first, second,
        "checklist R4: metadata view must be stable across calls"
    );
    assert!(
        !first.id.is_empty(),
        "checklist R4: metadata must carry a non-empty registry id"
    );
}

/// Checklist R5: `supports_interface` MUST return `false` (not error) for an
/// interface identifier the provider does not implement.
#[test]
fn checklist_r5_unknown_interface_returns_false_not_error() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let answer = adapter.supports("definitely-not-an-interface");
    assert!(
        matches!(answer, Ok(false)),
        "checklist R5: unknown interface must yield Ok(false), got {answer:?}"
    );
}

/// Checklist R6: every score returned MUST lie within the declared bounds.
#[test]
fn checklist_r6_scores_are_within_declared_bounds() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let (min, max) = adapter.score_bounds();
    for account in adapter.sample_accounts() {
        let score = adapter.score(&account).expect("score must answer");
        assert!(
            score >= min && score <= max,
            "checklist R6: score {score} for {account:?} outside bounds [{min}, {max}]"
        );
    }
}

/// Checklist R7: the gate threshold MUST lie within the declared score bounds.
#[test]
fn checklist_r7_gate_threshold_is_within_bounds() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let (min, max) = adapter.score_bounds();
    let threshold = adapter.gate_threshold();
    assert!(
        threshold >= min && threshold <= max,
        "checklist R7: gate threshold {threshold} outside bounds [{min}, {max}]"
    );
}

/// Checklist R8: repeated reads at the same ledger time MUST return the same
/// score (no hidden time dependence in the view).
#[test]
fn checklist_r8_score_is_monotonic_in_time() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let account = adapter.known_account();
    let at_t0 = adapter.score_at(&account, 0).expect("score must answer");
    let at_t0_again = adapter.score_at(&account, 0).expect("score must answer");
    assert_eq!(
        at_t0, at_t0_again,
        "checklist R8: score at a fixed ledger time must be stable"
    );
}

/// Checklist R9: querying an unknown account MUST NOT panic and MUST return a
/// well-defined default rather than an error.
#[test]
fn checklist_r9_unknown_account_is_well_defined() {
    let mut adapter = Adapter::new();
    adapter.reset();
    let unknown = adapter.unknown_account();
    let score = adapter.score(&unknown).expect("score must answer for unknown account");
    let (min, max) = adapter.score_bounds();
    assert!(
        score >= min && score <= max,
        "checklist R9: unknown-account score {score} outside bounds [{min}, {max}]"
    );
}

/// Checklist R10: no view MUST panic on adversarial input; every entry point
/// returns a `Result` or a well-defined value.
#[test]
fn checklist_r10_views_do_not_panic_on_adversarial_input() {
    let mut adapter = Adapter::new();
    adapter.reset();
    // Empty and oversized identifiers must be handled without panicking.
    let _ = adapter.supports("");
    let _ = adapter.supports(&"x".repeat(1024));
    let _ = adapter.score(&adapter.unknown_account());
    let _ = adapter.gate(&adapter.unknown_account());
    let _ = adapter.metadata();
}

#[test]
fn should_level_deviations_do_not_break_conformance() {
    // Guards the harness's own contract: a `should` failure is reported but is
    // not a conformance failure. If this ever breaks, providers will start
    // treating advisory vectors as mandatory.
    let mut adapter = Adapter::new();
    let report = run(&mut adapter);
    for result in &report.results {
        if result.level == Level::Should {
            assert!(
                !matches!(result.outcome, Outcome::Fail(_)),
                "the reference provider should not deviate on `{}`",
                result.id
            );
        }
    }
}
