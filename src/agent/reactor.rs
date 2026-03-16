use serde::{Deserialize, Serialize};

use crate::agent::claude_executor::SpecialistResult;
use crate::agent::knowledge::KnowledgeBase;

// ---------------------------------------------------------------------------
// Intel events — typed signals extracted from specialist results
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IntelEvent {
    CredentialFound,
    AccessGained,
    ObjectiveCaptured,
    AttackSurfaceExpanded,
    IntelFound,
    VulnerabilityConfirmed,
    TaskFailed,
    HypothesisConfirmed { hypothesis_id: String },
    HypothesisRefuted { hypothesis_id: String },
    ModelingComplete,
    StackIdentified { component_id: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Urgency {
    InterruptAndReplan,
    InjectAndContinue,
    NoteAndContinue,
}

// ---------------------------------------------------------------------------
// classify_events — inspect a SpecialistResult and produce typed events
// ---------------------------------------------------------------------------

pub fn classify_events(
    result: &SpecialistResult,
    _kb: &KnowledgeBase,
) -> Vec<(IntelEvent, Urgency)> {
    let mut events = Vec::new();

    if !result.credentials.is_empty() {
        events.push((IntelEvent::CredentialFound, Urgency::InterruptAndReplan));
    }

    if !result.access_levels.is_empty() {
        events.push((IntelEvent::AccessGained, Urgency::InterruptAndReplan));
    }

    if !result.flags.is_empty() {
        events.push((IntelEvent::ObjectiveCaptured, Urgency::NoteAndContinue));
    }

    if !result.discovered_hosts.is_empty() {
        events.push((
            IntelEvent::AttackSurfaceExpanded,
            Urgency::InjectAndContinue,
        ));
    }

    for finding in &result.findings {
        let sev = finding.severity.to_lowercase();
        if sev == "critical" || sev == "high" {
            events.push((
                IntelEvent::VulnerabilityConfirmed,
                Urgency::InterruptAndReplan,
            ));
        } else {
            events.push((IntelEvent::IntelFound, Urgency::NoteAndContinue));
        }
    }

    if !result.notes.is_empty()
        && events.is_empty()
        && result.credentials.is_empty()
        && result.flags.is_empty()
        && result.discovered_hosts.is_empty()
        && result.findings.is_empty()
        && result.access_levels.is_empty()
    {
        events.push((IntelEvent::IntelFound, Urgency::NoteAndContinue));
    }

    events
}

// ---------------------------------------------------------------------------
// react — turn classified events into notes (simplified for advisor model)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default)]
pub struct ReactorOutput {
    pub should_replan: bool,
    pub notes: Vec<String>,
    pub cancel_hypothesis_ids: Vec<String>,
}

pub fn react(events: &[(IntelEvent, Urgency)], _kb: &KnowledgeBase) -> ReactorOutput {
    let mut output = ReactorOutput::default();

    for (event, urgency) in events {
        match urgency {
            Urgency::InterruptAndReplan => {
                output.should_replan = true;
            }
            Urgency::InjectAndContinue | Urgency::NoteAndContinue => {}
        }

        match event {
            IntelEvent::CredentialFound => {
                output.notes.push(
                    "New credential discovered — consider spraying across known services."
                        .to_string(),
                );
            }
            IntelEvent::AccessGained => {
                output
                    .notes
                    .push("New access level gained — consider post-exploitation.".to_string());
            }
            IntelEvent::ObjectiveCaptured => {
                output.notes.push("Flag or objective captured.".to_string());
            }
            IntelEvent::AttackSurfaceExpanded => {
                output
                    .notes
                    .push("New hosts discovered — attack surface expanded.".to_string());
            }
            IntelEvent::VulnerabilityConfirmed => {
                output.notes.push(
                    "High/critical vulnerability confirmed — prioritize exploitation.".to_string(),
                );
            }
            IntelEvent::IntelFound => {
                output.notes.push("New intelligence gathered.".to_string());
            }
            IntelEvent::TaskFailed => {
                output
                    .notes
                    .push("Task failed — may need alternative approach.".to_string());
            }
            IntelEvent::HypothesisConfirmed { hypothesis_id } => {
                output.notes.push(format!(
                    "Hypothesis {hypothesis_id} confirmed — consider exploitation."
                ));
            }
            IntelEvent::HypothesisRefuted { hypothesis_id } => {
                output.cancel_hypothesis_ids.push(hypothesis_id.clone());
                output
                    .notes
                    .push(format!("Hypothesis {hypothesis_id} refuted — move on."));
            }
            IntelEvent::ModelingComplete => {
                output.notes.push(
                    "System modeling complete — advancing to hypothesis generation.".to_string(),
                );
            }
            IntelEvent::StackIdentified { component_id } => {
                output
                    .notes
                    .push(format!("Stack identified for component {component_id}."));
            }
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::claude_executor::{CredentialResult, FindingReport};

    #[test]
    fn classify_credential_result() {
        let result = SpecialistResult {
            credentials: vec![CredentialResult {
                username: "admin".into(),
                password: Some("pass123".into()),
                hash: None,
                service: "ssh".into(),
                host: "10.0.0.1".into(),
            }],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();

        let events = classify_events(&result, &kb);
        assert!(events.contains(&(IntelEvent::CredentialFound, Urgency::InterruptAndReplan)));
    }

    #[test]
    fn classify_flag_result() {
        let result = SpecialistResult {
            flags: vec!["FLAG{test_flag_123}".into()],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();

        let events = classify_events(&result, &kb);
        assert!(events.contains(&(IntelEvent::ObjectiveCaptured, Urgency::NoteAndContinue)));
    }

    #[test]
    fn empty_result_produces_no_events() {
        let result = SpecialistResult::default();
        let kb = KnowledgeBase::new();

        let events = classify_events(&result, &kb);
        assert!(events.is_empty());
    }

    #[test]
    fn react_sets_replan_on_interrupt_urgency() {
        let events = vec![(IntelEvent::CredentialFound, Urgency::InterruptAndReplan)];
        let kb = KnowledgeBase::new();

        let output = react(&events, &kb);
        assert!(output.should_replan);
        assert!(!output.notes.is_empty());
    }

    #[test]
    fn react_no_replan_on_note_urgency() {
        let events = vec![(IntelEvent::ObjectiveCaptured, Urgency::NoteAndContinue)];
        let kb = KnowledgeBase::new();

        let output = react(&events, &kb);
        assert!(!output.should_replan);
    }

    #[test]
    fn classify_high_severity_finding() {
        let result = SpecialistResult {
            findings: vec![FindingReport {
                vuln_type: "SQL Injection".into(),
                severity: "Critical".into(),
                endpoint: "/api/login".into(),
                description: "SQL injection in login form".into(),
                evidence: "' OR 1=1 --".into(),
            }],
            ..Default::default()
        };
        let kb = KnowledgeBase::new();

        let events = classify_events(&result, &kb);
        assert!(events.contains(&(
            IntelEvent::VulnerabilityConfirmed,
            Urgency::InterruptAndReplan
        )));
    }

    #[test]
    fn react_refuted_hypothesis_adds_cancel_id() {
        let kb = KnowledgeBase::new();
        let events = vec![(
            IntelEvent::HypothesisRefuted {
                hypothesis_id: "h-99".into(),
            },
            Urgency::NoteAndContinue,
        )];

        let output = react(&events, &kb);
        assert_eq!(output.cancel_hypothesis_ids, vec!["h-99".to_string()]);
    }
}
