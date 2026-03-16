//! # Deductive Metrics Tests
//!
//! Redtrail's value proposition: "deduction over enumeration."
//! These tests validate the metrics that prove deductive reasoning
//! is actually more efficient than brute-force scanning.
//!
//! Key metrics:
//! - efficiency_score: probe_calls / total_tool_calls (higher = more deductive)
//! - brute_force_ratio: brute_force_calls / total_tool_calls (lower = better)
//! - confirmation_rate: hypotheses_confirmed / hypotheses_generated
//! - flags_per_call: flags_captured / total_tool_calls

use redtrail::agent::knowledge::DeductiveMetrics;

// ===========================================================================
// Efficiency score (probe ratio)
// ===========================================================================

/// A purely deductive session (all probes, no brute force) should have
/// efficiency score of 1.0.
#[test]
fn purely_deductive_session_has_max_efficiency() {
    let metrics = DeductiveMetrics {
        total_tool_calls: 15,
        probe_calls: 15,
        brute_force_calls: 0,
        enumeration_calls: 0,
        hypotheses_generated: 5,
        hypotheses_confirmed: 3,
        hypotheses_refuted: 2,
        flags_captured: 3,
        wall_clock_secs: 120,
    };

    assert_eq!(metrics.efficiency_score(), 1.0);
    assert_eq!(metrics.brute_force_ratio(), 0.0);
}

/// A purely brute-force session (no probes) should have efficiency 0.0.
#[test]
fn purely_brute_force_session_has_zero_efficiency() {
    let metrics = DeductiveMetrics {
        total_tool_calls: 100,
        probe_calls: 0,
        brute_force_calls: 100,
        enumeration_calls: 0,
        hypotheses_generated: 0,
        hypotheses_confirmed: 0,
        hypotheses_refuted: 0,
        flags_captured: 1,
        wall_clock_secs: 600,
    };

    assert_eq!(metrics.efficiency_score(), 0.0);
    assert_eq!(metrics.brute_force_ratio(), 1.0);
}

/// The PRD targets probe_ratio > 0.7 for web-focused modules.
/// Validate that a realistic deductive session meets this threshold.
#[test]
fn realistic_deductive_session_meets_threshold() {
    let metrics = DeductiveMetrics {
        total_tool_calls: 30,
        probe_calls: 22,      // 73% probes
        brute_force_calls: 2, // 7% brute force
        enumeration_calls: 6, // 20% enumeration
        hypotheses_generated: 8,
        hypotheses_confirmed: 5,
        hypotheses_refuted: 3,
        flags_captured: 4,
        wall_clock_secs: 180,
    };

    assert!(
        metrics.efficiency_score() > 0.7,
        "Deductive session should exceed 0.7 probe ratio, got {}",
        metrics.efficiency_score()
    );
    assert!(
        metrics.brute_force_ratio() < 0.1,
        "Deductive session should have < 10% brute force, got {}",
        metrics.brute_force_ratio()
    );
}

// ===========================================================================
// Confirmation rate
// ===========================================================================

/// Confirmation rate measures hypothesis quality.
/// Good hypotheses should have a reasonable confirmation rate.
#[test]
fn confirmation_rate_calculation() {
    let metrics = DeductiveMetrics {
        hypotheses_generated: 10,
        hypotheses_confirmed: 4,
        hypotheses_refuted: 6,
        ..Default::default()
    };

    assert!((metrics.confirmation_rate() - 0.4).abs() < f64::EPSILON);
}

/// Zero hypotheses generated should return 0.0, not NaN or panic.
#[test]
fn confirmation_rate_zero_hypotheses() {
    let metrics = DeductiveMetrics::default();
    assert_eq!(metrics.confirmation_rate(), 0.0);
}

/// Zero total calls should return 0.0 for all ratios.
#[test]
fn zero_calls_returns_zero_ratios() {
    let metrics = DeductiveMetrics::default();
    assert_eq!(metrics.efficiency_score(), 0.0);
    assert_eq!(metrics.brute_force_ratio(), 0.0);
    assert_eq!(metrics.flags_per_call(), 0.0);
    assert_eq!(metrics.confirmation_rate(), 0.0);
}

// ===========================================================================
// Flags per call (deductive efficiency)
// ===========================================================================

/// Deductive approach: fewer calls to capture more flags.
#[test]
fn flags_per_call_deductive_vs_brute_force() {
    let deductive = DeductiveMetrics {
        total_tool_calls: 25,
        probe_calls: 18,
        brute_force_calls: 0,
        flags_captured: 4,
        ..Default::default()
    };

    let brute_force = DeductiveMetrics {
        total_tool_calls: 200,
        probe_calls: 0,
        brute_force_calls: 180,
        flags_captured: 4,
        ..Default::default()
    };

    assert!(
        deductive.flags_per_call() > brute_force.flags_per_call(),
        "Deductive ({:.3}) should capture more flags per call than brute force ({:.3})",
        deductive.flags_per_call(),
        brute_force.flags_per_call()
    );

    // Deductive: 4/25 = 0.16, Brute force: 4/200 = 0.02 — 8x more efficient
    let efficiency_gain = deductive.flags_per_call() / brute_force.flags_per_call();
    assert!(
        efficiency_gain > 5.0,
        "Deductive should be at least 5x more efficient per call, got {:.1}x",
        efficiency_gain
    );
}

// ===========================================================================
// Metrics coherence: invariants that should always hold
// ===========================================================================

/// Probe calls + brute force calls <= total tool calls.
#[test]
fn metrics_coherence_subcategories_dont_exceed_total() {
    let metrics = DeductiveMetrics {
        total_tool_calls: 50,
        probe_calls: 30,
        brute_force_calls: 10,
        enumeration_calls: 10,
        ..Default::default()
    };

    assert!(
        metrics.probe_calls + metrics.brute_force_calls + metrics.enumeration_calls
            <= metrics.total_tool_calls,
        "Subcategory calls should not exceed total"
    );
}

/// Confirmed + refuted <= generated.
#[test]
fn metrics_coherence_hypothesis_counts() {
    let metrics = DeductiveMetrics {
        hypotheses_generated: 10,
        hypotheses_confirmed: 4,
        hypotheses_refuted: 5,
        ..Default::default()
    };

    assert!(
        metrics.hypotheses_confirmed + metrics.hypotheses_refuted <= metrics.hypotheses_generated,
        "Confirmed + refuted should not exceed generated"
    );
}

/// Efficiency score is always between 0.0 and 1.0.
#[test]
fn efficiency_score_bounded() {
    let cases = vec![
        DeductiveMetrics::default(),
        DeductiveMetrics {
            total_tool_calls: 100,
            probe_calls: 100,
            ..Default::default()
        },
        DeductiveMetrics {
            total_tool_calls: 100,
            probe_calls: 0,
            ..Default::default()
        },
        DeductiveMetrics {
            total_tool_calls: 1,
            probe_calls: 1,
            ..Default::default()
        },
    ];

    for metrics in cases {
        let score = metrics.efficiency_score();
        assert!(
            (0.0..=1.0).contains(&score),
            "Efficiency score must be in [0, 1], got {}",
            score
        );
    }
}
