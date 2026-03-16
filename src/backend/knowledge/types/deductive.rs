use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum DeductiveLayer {
    #[default]
    Modeling,
    Hypothesizing,
    Probing,
    Exploiting,
    Chaining,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct DeductiveMetrics {
    #[serde(default)]
    pub total_tool_calls: u32,
    #[serde(default)]
    pub probe_calls: u32,
    #[serde(default)]
    pub brute_force_calls: u32,
    #[serde(default)]
    pub enumeration_calls: u32,
    #[serde(default)]
    pub hypotheses_generated: u32,
    #[serde(default)]
    pub hypotheses_confirmed: u32,
    #[serde(default)]
    pub hypotheses_refuted: u32,
    #[serde(default)]
    pub flags_captured: u32,
    #[serde(default)]
    pub wall_clock_secs: u64,
}

impl DeductiveMetrics {
    pub fn efficiency_score(&self) -> f64 {
        if self.total_tool_calls == 0 {
            return 0.0;
        }
        self.probe_calls as f64 / self.total_tool_calls as f64
    }

    pub fn flags_per_call(&self) -> f64 {
        if self.total_tool_calls == 0 {
            return 0.0;
        }
        self.flags_captured as f64 / self.total_tool_calls as f64
    }

    pub fn confirmation_rate(&self) -> f64 {
        if self.hypotheses_generated == 0 {
            return 0.0;
        }
        self.hypotheses_confirmed as f64 / self.hypotheses_generated as f64
    }

    pub fn brute_force_ratio(&self) -> f64 {
        if self.total_tool_calls == 0 {
            return 0.0;
        }
        self.brute_force_calls as f64 / self.total_tool_calls as f64
    }
}
