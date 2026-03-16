//! # Defender Model Tests
//!
//! Redtrail's deductive approach includes modeling the defender:
//! WAFs, IDS, rate limits, and noise budgets. A good pentester
//! adapts their approach based on detected defenses.
//!
//! These tests validate:
//! 1. WAF detection from blocked payloads
//! 2. Noise budget reduction under detection
//! 3. Action allowance based on noise budget
//! 4. Bypass technique suggestions
//! 5. Adjusted priority under defender awareness

use redtrail::agent::knowledge::{DefenderModel, IdsSensitivity, WafType};

// ===========================================================================
// WAF detection from blocked payloads
// ===========================================================================

/// Recording a block should auto-detect WAF and add it to the model.
#[test]
fn record_block_creates_waf_entry() {
    let mut defender = DefenderModel::default();
    assert!(defender.detected_wafs.is_empty());

    defender.record_block("10.0.0.1", "' OR 1=1--", 403);

    assert_eq!(defender.detected_wafs.len(), 1);
    assert_eq!(defender.detected_wafs[0].host, "10.0.0.1");
    assert_eq!(defender.detected_wafs[0].confidence, 0.5);
    assert_eq!(defender.detected_wafs[0].blocked_payloads.len(), 1);
}

/// Multiple blocks on same host increase WAF confidence.
#[test]
fn multiple_blocks_increase_confidence() {
    let mut defender = DefenderModel::default();

    defender.record_block("10.0.0.1", "' OR 1=1--", 403);
    defender.record_block("10.0.0.1", "<script>alert(1)</script>", 403);
    defender.record_block("10.0.0.1", "../../etc/passwd", 403);

    assert_eq!(defender.detected_wafs.len(), 1);
    assert!(
        defender.detected_wafs[0].confidence > 0.5,
        "Confidence should increase with more blocks, got {}",
        defender.detected_wafs[0].confidence
    );
    assert_eq!(defender.detected_wafs[0].blocked_payloads.len(), 3);
}

/// WAF confidence is capped at 1.0.
#[test]
fn waf_confidence_capped_at_one() {
    let mut defender = DefenderModel::default();

    // Record many blocks
    for i in 0..20 {
        defender.record_block("10.0.0.1", &format!("payload-{i}"), 403);
    }

    assert!(
        defender.detected_wafs[0].confidence <= 1.0,
        "Confidence should not exceed 1.0, got {}",
        defender.detected_wafs[0].confidence
    );
}

/// Different HTTP status codes indicate different WAF types.
#[test]
fn waf_type_classified_from_status() {
    let mut defender = DefenderModel::default();

    defender.record_block("host-406", "payload", 406);
    defender.record_block("host-403", "payload", 403);
    defender.record_block("host-429", "payload", 429);

    let waf_406 = defender
        .detected_wafs
        .iter()
        .find(|w| w.host == "host-406")
        .unwrap();
    assert!(matches!(waf_406.waf_type, WafType::ModSecurity));

    let waf_403 = defender
        .detected_wafs
        .iter()
        .find(|w| w.host == "host-403")
        .unwrap();
    assert!(matches!(waf_403.waf_type, WafType::Unknown(ref s) if s == "generic_403"));

    let waf_429 = defender
        .detected_wafs
        .iter()
        .find(|w| w.host == "host-429")
        .unwrap();
    assert!(matches!(waf_429.waf_type, WafType::Unknown(ref s) if s == "rate_limiter"));
}

// ===========================================================================
// Noise budget
// ===========================================================================

/// Fresh defender model has full noise budget (1.0).
#[test]
fn fresh_model_has_full_noise_budget() {
    let defender = DefenderModel::default();
    assert_eq!(defender.noise_budget, 1.0);
}

/// Each blocked payload reduces the noise budget.
#[test]
fn blocks_reduce_noise_budget() {
    let mut defender = DefenderModel::default();

    defender.record_block("10.0.0.1", "payload1", 403);
    assert!(
        defender.noise_budget < 1.0,
        "Noise budget should decrease after block, got {}",
        defender.noise_budget
    );

    let budget_after_first = defender.noise_budget;
    defender.record_block("10.0.0.1", "payload2", 403);
    assert!(
        defender.noise_budget < budget_after_first,
        "Noise budget should continue decreasing"
    );
}

/// Noise budget floor is 0.0.
#[test]
fn noise_budget_floor_at_zero() {
    let mut defender = DefenderModel::default();

    for i in 0..20 {
        defender.record_block("10.0.0.1", &format!("payload-{i}"), 403);
    }

    assert!(
        defender.noise_budget >= 0.0,
        "Noise budget should not go below 0.0, got {}",
        defender.noise_budget
    );
}

// ===========================================================================
// Action allowance based on noise budget
// ===========================================================================

/// With full noise budget, all actions should be allowed.
#[test]
fn full_budget_allows_all_actions() {
    let defender = DefenderModel::default();

    assert!(defender.is_action_allowed("differential_probe"));
    assert!(defender.is_action_allowed("port_scan"));
    assert!(defender.is_action_allowed("sql_injection"));
    assert!(defender.is_action_allowed("brute_force"));
}

/// With zero noise budget, only silent actions should be allowed.
#[test]
fn zero_budget_blocks_noisy_actions() {
    let mut defender = DefenderModel::default();
    defender.noise_budget = 0.0;

    // These should be blocked (detection cost > 0)
    assert!(
        !defender.is_action_allowed("brute_force"),
        "Brute force (0.8 cost) should be blocked at 0 budget"
    );
    assert!(
        !defender.is_action_allowed("sql_injection"),
        "SQL injection (0.5 cost) should be blocked at 0 budget"
    );
    assert!(
        !defender.is_action_allowed("dir_bust"),
        "Dir bust (0.3 cost) should be blocked at 0 budget"
    );
}

/// Detection cost ordering: probes < scans < exploits < brute force.
#[test]
fn detection_cost_ordering() {
    let defender = DefenderModel::default();

    let probe_cost = defender.detection_cost_for("differential_probe");
    let scan_cost = defender.detection_cost_for("port_scan");
    let exploit_cost = defender.detection_cost_for("sql_injection");
    let brute_cost = defender.detection_cost_for("brute_force");

    assert!(
        probe_cost < scan_cost,
        "Probes ({probe_cost}) should be quieter than scans ({scan_cost})"
    );
    assert!(
        scan_cost < exploit_cost,
        "Scans ({scan_cost}) should be quieter than exploits ({exploit_cost})"
    );
    assert!(
        exploit_cost < brute_cost,
        "Exploits ({exploit_cost}) should be quieter than brute force ({brute_cost})"
    );
}

/// Post-access actions should have very low detection cost
/// (you're already on the box).
#[test]
fn post_access_actions_low_cost() {
    let defender = DefenderModel::default();

    let privesc_cost = defender.detection_cost_for("privesc_enum");
    let read_flag_cost = defender.detection_cost_for("read_flag");

    assert!(
        privesc_cost < 0.1,
        "Privesc enum should be very quiet (post-access), got {}",
        privesc_cost
    );
    assert!(
        read_flag_cost < 0.1,
        "Read flag should be very quiet (post-access), got {}",
        read_flag_cost
    );
}

// ===========================================================================
// Bypass techniques
// ===========================================================================

/// Default bypasses should be populated.
#[test]
fn default_bypasses_available() {
    let defender = DefenderModel::with_default_bypasses();
    assert!(
        !defender.bypass_techniques.is_empty(),
        "Should have default bypass techniques"
    );
}

/// Bypass suggestions should match the detected WAF type.
#[test]
fn bypass_suggestions_match_waf_type() {
    let mut defender = DefenderModel::with_default_bypasses();

    // Detect a ModSecurity WAF
    defender.record_block("10.0.0.1", "' OR 1=1--", 406);

    let suggestions = defender.suggest_bypasses("10.0.0.1");
    assert!(
        !suggestions.is_empty(),
        "Should suggest bypasses for ModSecurity WAF"
    );

    // All suggestions should be effective against ModSecurity or universal
    for bypass in &suggestions {
        let is_applicable = bypass.effective_against.contains(&WafType::ModSecurity)
            || bypass.effective_against.is_empty();
        assert!(
            is_applicable,
            "Bypass '{}' should be effective against ModSecurity or universal",
            bypass.name
        );
    }
}

/// No bypass suggestions for a host without a detected WAF.
#[test]
fn no_bypass_suggestions_without_waf() {
    let defender = DefenderModel::with_default_bypasses();
    let suggestions = defender.suggest_bypasses("10.0.0.99");
    assert!(
        suggestions.is_empty(),
        "No WAF detected on host → no bypass suggestions"
    );
}

/// Recording a successful bypass should update the WAF entry.
#[test]
fn record_bypass_updates_waf() {
    let mut defender = DefenderModel::default();
    defender.record_block("10.0.0.1", "payload", 403);

    defender.record_bypass("10.0.0.1", "case_alternation");

    let waf = &defender.detected_wafs[0];
    assert!(
        waf.successful_bypasses
            .contains(&"case_alternation".to_string()),
        "WAF should record successful bypass"
    );
}

// ===========================================================================
// Adjusted priority (defender-aware task scheduling)
// ===========================================================================

/// With full noise budget, adjusted priority equals base priority.
#[test]
fn full_budget_no_priority_adjustment() {
    let defender = DefenderModel::default();
    let adjusted = defender.adjusted_priority(75.0, "sql_injection");
    assert_eq!(adjusted, 75.0, "Full budget should not reduce priority");
}

/// With reduced noise budget, noisy actions get deprioritized.
#[test]
fn reduced_budget_penalizes_noisy_actions() {
    let mut defender = DefenderModel::default();
    defender.noise_budget = 0.5;

    let brute_force_priority = defender.adjusted_priority(75.0, "brute_force");
    let probe_priority = defender.adjusted_priority(75.0, "differential_probe");

    assert!(
        probe_priority > brute_force_priority,
        "Probes ({probe_priority}) should be prioritized over brute force ({brute_force_priority}) when budget is low"
    );
}

/// With zero noise budget, noisy actions get maximum penalty.
#[test]
fn zero_budget_maximum_penalty() {
    let mut defender = DefenderModel::default();
    defender.noise_budget = 0.0;

    let brute_force_priority = defender.adjusted_priority(75.0, "brute_force");

    assert!(
        brute_force_priority < 75.0,
        "Brute force should be heavily penalized at 0 budget, got {}",
        brute_force_priority
    );
}

// ===========================================================================
// Rate limit recording
// ===========================================================================

/// Rate limits should be recorded and queryable.
#[test]
fn rate_limit_recording() {
    let mut defender = DefenderModel::default();
    defender.record_rate_limit("10.0.0.1", Some("/api/login"), 10, 60, 429);

    assert_eq!(defender.rate_limits.len(), 1);
    assert_eq!(defender.rate_limits[0].host, "10.0.0.1");
    assert_eq!(defender.rate_limits[0].max_requests, 10);
    assert_eq!(defender.rate_limits[0].window_secs, 60);
    assert_eq!(defender.rate_limits[0].limit_status, 429);
}

/// IDS sensitivity can be updated.
#[test]
fn ids_sensitivity_update() {
    let mut defender = DefenderModel::default();
    assert_eq!(defender.ids_sensitivity, IdsSensitivity::None);

    defender.update_ids_sensitivity(IdsSensitivity::High);
    assert_eq!(defender.ids_sensitivity, IdsSensitivity::High);
}
