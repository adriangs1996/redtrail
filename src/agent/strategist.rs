use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::agent::knowledge::{
    ComponentType, ComponentUpdate, DataFlow, DeductiveLayer, EntryPoint, GoalStatus, Hypothesis,
    HypothesisCategory, HypothesisStatus, KnowledgeBase, SystemComponent, SystemModel,
    TrustBoundary,
};
use crate::agent::llm::LlmProvider;
use crate::db::{CrossSessionIntel, RelevanceQuery, Db};
use crate::error::Error;

/// Build a `RelevanceQuery` from the KB's discovered services and technologies.
pub fn build_relevance_query(kb: &KnowledgeBase) -> Option<RelevanceQuery> {
    if kb.discovered_hosts.is_empty() {
        return None;
    }

    let mut services: Vec<String> = kb
        .discovered_hosts
        .iter()
        .flat_map(|h| h.services.iter().cloned())
        .collect();
    services.sort();
    services.dedup();

    let mut technologies: Vec<String> = kb
        .system_model
        .components
        .iter()
        .flat_map(|c| {
            let mut techs = c.stack.technologies.clone();
            if let Some(ref s) = c.stack.server {
                techs.push(s.clone());
            }
            if let Some(ref f) = c.stack.framework {
                techs.push(f.clone());
            }
            if let Some(ref l) = c.stack.language {
                techs.push(l.clone());
            }
            techs
        })
        .collect();
    technologies.sort();
    technologies.dedup();

    let goal_type = Some(match &kb.goal.goal_type {
        crate::agent::knowledge::GoalType::CaptureFlags { .. } => "CaptureFlags".to_string(),
        crate::agent::knowledge::GoalType::GainAccess { .. } => "GainAccess".to_string(),
        crate::agent::knowledge::GoalType::Exfiltrate { .. } => "Exfiltrate".to_string(),
        crate::agent::knowledge::GoalType::VulnerabilityAssessment { .. } => {
            "VulnerabilityAssessment".to_string()
        }
        crate::agent::knowledge::GoalType::Custom { .. } => "Custom".to_string(),
    });

    if services.is_empty() && technologies.is_empty() {
        return None;
    }

    Some(RelevanceQuery {
        services,
        technologies,
        goal_type,
        tags: vec![],
    })
}

/// Query cross-session DB for relevant intel based on current KB state.
pub fn query_cross_session_intel(kb: &KnowledgeBase, db: &Db) -> Option<CrossSessionIntel> {
    let query = build_relevance_query(kb)?;

    match db.gather_cross_session_intel(&query) {
        Ok(intel) => {
            if intel.relevant_patterns.is_empty()
                && intel.similar_sessions.is_empty()
                && intel.technique_stats.is_empty()
            {
                None
            } else {
                Some(intel)
            }
        }
        Err(e) => {
            eprintln!("[strategist] cross-session intel query failed: {e}");
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Priority — local definition (was in task_tree, now kept here for strategist)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Critical,
    High,
    Medium,
    Low,
    Background,
}

impl Priority {
    pub fn weight(&self) -> u32 {
        match self {
            Priority::Critical => 100,
            Priority::High => 75,
            Priority::Medium => 50,
            Priority::Low => 25,
            Priority::Background => 10,
        }
    }
}

// ---------------------------------------------------------------------------
// TaskDefinition — simplified local version for strategist output
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    pub name: String,
    pub description: String,
    pub prompt_template: String,
    #[serde(default)]
    pub applicable_when: Vec<String>,
    #[serde(default)]
    pub expected_output: Vec<String>,
    #[serde(default)]
    pub default_priority: u32,
    #[serde(default)]
    pub estimated_duration_secs: u64,
    #[serde(default)]
    pub builtin: bool,
    #[serde(default)]
    pub reusable: bool,
}

// ---------------------------------------------------------------------------
// Strategist output types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedTask {
    pub definition_name: String,
    pub params: HashMap<String, String>,
    pub priority: Priority,
    pub rationale: String,
    pub confidence: f32,
}

impl ProposedTask {
    /// Compute effective score: `priority_weight * confidence`.
    pub fn effective_score(&self) -> f32 {
        self.priority.weight() as f32 * self.confidence
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelFilter {
    #[serde(default)]
    pub definition_name: Option<String>,
    #[serde(default)]
    pub host: Option<String>,
    #[serde(default)]
    pub reason: String,
}

/// An update to apply to the system model, returned by the strategist.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ModelUpdate {
    AddComponent(SystemComponent),
    UpdateComponent {
        id: String,
        updates: ComponentUpdate,
    },
    AddBoundary(TrustBoundary),
    AddDataFlow(DataFlow),
    AddEntryPoint {
        component_id: String,
        entry_point: EntryPoint,
    },
}

/// A request from the strategist for deeper cross-session memory lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryQuery {
    pub query_type: String,
    #[serde(default)]
    pub services: Vec<String>,
    #[serde(default)]
    pub technologies: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategistPlan {
    pub assessment: String,
    pub tasks: Vec<ProposedTask>,
    pub new_definitions: Vec<TaskDefinition>,
    #[serde(default)]
    pub cancel: Vec<CancelFilter>,
    pub is_complete: bool,
    #[serde(default)]
    pub memory_query: Option<MemoryQuery>,
    #[serde(default)]
    pub hypotheses: Vec<Hypothesis>,
    #[serde(default, deserialize_with = "deserialize_model_updates_lenient")]
    pub model_updates: Vec<ModelUpdate>,
    #[serde(default)]
    pub advance_layer: Option<DeductiveLayer>,
}

/// Deserialize model_updates leniently — skip entries that fail to parse.
fn deserialize_model_updates_lenient<'de, D>(deserializer: D) -> Result<Vec<ModelUpdate>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let values: Vec<serde_json::Value> = Vec::deserialize(deserializer)?;
    Ok(values
        .into_iter()
        .filter_map(|v| match serde_json::from_value::<ModelUpdate>(v) {
            Ok(update) => Some(update),
            Err(e) => {
                eprintln!("[strategist] skipping invalid model_update: {e}");
                None
            }
        })
        .collect())
}

// ---------------------------------------------------------------------------
// Suggestion types — the new advisor output format
// ---------------------------------------------------------------------------

/// A single suggestion from the AI advisor to the human pentester.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    /// Human-readable action description.
    pub action: String,
    /// Exact command(s) the user could run (e.g., "!nmap -sV -p22 10.10.10.1").
    #[serde(default)]
    pub commands: Vec<String>,
    /// Why this is recommended — references KB data, deductive reasoning.
    pub rationale: String,
    /// BISCL category if testing a hypothesis.
    #[serde(default)]
    pub category: Option<HypothesisCategory>,
    /// What we expect to learn or gain.
    #[serde(default)]
    pub expected_yield: Option<String>,
    /// Priority level.
    pub priority: Priority,
    /// Confidence (0.0-1.0).
    pub confidence: f32,
}

impl Suggestion {
    /// Effective score = priority_weight * confidence.
    pub fn effective_score(&self) -> f32 {
        self.priority.weight() as f32 * self.confidence
    }
}

/// The advisor's full response — replaces StrategistPlan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvisorSuggestion {
    pub assessment: String,
    #[serde(default)]
    pub suggestions: Vec<Suggestion>,
    #[serde(default)]
    pub hypotheses: Vec<Hypothesis>,
    #[serde(default, deserialize_with = "deserialize_model_updates_lenient")]
    pub model_updates: Vec<ModelUpdate>,
    #[serde(default)]
    pub advance_layer: Option<DeductiveLayer>,
    #[serde(default)]
    pub memory_query: Option<MemoryQuery>,
}

// ---------------------------------------------------------------------------
// Strategist — LLM-based strategic reasoning (no tools)
// ---------------------------------------------------------------------------

pub struct Strategist {
    provider: std::sync::Arc<dyn LlmProvider>,
}

const STRATEGY_MARKER: &str = "===REDTRAIL_STRATEGY===";

impl Strategist {
    pub fn new(provider: std::sync::Arc<dyn LlmProvider>) -> Self {
        Self { provider }
    }

    /// Call the LLM strategist with KB state and return advisor suggestions.
    pub async fn advise(&self, kb: &KnowledgeBase) -> Result<AdvisorSuggestion, Error> {
        self.advise_with_intel(kb, None).await
    }

    /// Call the LLM strategist with KB state and optional cross-session intelligence.
    pub async fn advise_with_intel(
        &self,
        kb: &KnowledgeBase,
        intel: Option<&CrossSessionIntel>,
    ) -> Result<AdvisorSuggestion, Error> {
        let system_prompt = build_system_prompt();
        let user_message = build_user_message(kb, intel);

        let prompt = format!("{system_prompt}\n\n{user_message}");

        let messages = vec![crate::agent::llm::ChatMessage::user(prompt)];
        let stream = self
            .provider
            .chat(&messages, &[], None)
            .await
            .map_err(|e| Error::Parse(format!("LLM call failed: {e}")))?;

        let resp = crate::agent::llm::collect_chat_response(stream)
            .await
            .map_err(|e| Error::Parse(format!("LLM stream failed: {e}")))?;

        let response = match resp {
            crate::agent::llm::ChatResponse::Text(t) => t,
            _ => return Err(Error::Parse("unexpected tool_use in advisor".into())),
        };

        parse_advisor_response(&response)
            .map_err(|e| Error::Parse(format!("Failed to parse advisor response: {e}")))
    }
}

// ---------------------------------------------------------------------------
// Prompt construction (THE BRAIN — preserved intact)
// ---------------------------------------------------------------------------

fn build_system_prompt() -> String {
    format!(
        r#"You are the ADVISOR for redtrail — a tool that assists a human pentester.
Your job is pure reasoning: analyse the current situation and suggest the next actions.
The human operator executes commands directly. You provide strategic guidance, suggest exact commands, and reason about the engagement.
You have NO tools. You cannot execute anything. You advise the human on what to do next.

# Deductive Protocol

You follow a structured 5-layer deductive reasoning process. Each layer builds on the previous one.
Progress through layers sequentially, but you may revisit earlier layers when new information emerges.

## L0 — Modeling (Fingerprint → Enumerate → Map)

Build a system model of the target before attacking:
1. **Fingerprint** each service: identify server software, framework, language, version via headers, error pages, response patterns. Use `StackFingerprint` tasks (5-8 requests max per service — DO NOT fuzz).
2. **Enumerate** entry points: discover paths, vhosts, parameters through directed techniques (directory discovery, sitemap parsing, robots.txt). This is targeted enumeration, NOT brute force.
3. **Map** architecture: identify components, trust boundaries, data flows between services. Use `MapArchitecture` tasks.

**Exit L0 when**: You have a system model with components, their stacks, and entry points identified. Model confidence should be ≥ 0.5.

## L1 — Hypothesizing (BISCL Categories)

Generate testable security hypotheses using the BISCL framework:
- **B**oundary: Trust boundary violations (e.g., "admin panel accessible without auth", "internal API exposed")
- **I**nput: Input handling flaws (e.g., "search parameter vulnerable to SQLi", "file upload allows path traversal", "OS command injection via parameter")
- **S**tate: State management issues (e.g., "session token predictable", "IDOR on user ID parameter")
- **C**onfidentiality: Information leakage (e.g., "debug mode exposes stack traces", "backup files accessible")
- **L**ogic: Business logic flaws (e.g., "price can be set to negative", "authentication bypass via parameter manipulation")

### Web Application Hypothesis Generation

For web applications, generate Input hypotheses for EVERY user-controlled parameter:
- **SQL Injection**: For each parameter used in database queries — test string params with `'`, numeric params with arithmetic (`1+1`, `2-1`), ORDER BY with column count. Consider: error-based, boolean-based blind, time-based blind, and UNION-based.
- **Command Injection**: For each parameter that may reach OS commands — test with command separators (`;`, `|`, `&&`, `||`), subshells (`$(cmd)`, `` `cmd` ``), and output-based detection (`id`, `whoami`).
- **Path Traversal**: For file-related parameters — test with `../`, encoded variants, null bytes.

Prioritize parameters that: appear in search/filter functionality (likely SQL), interact with file operations (likely CMDi/path traversal), or are reflected in responses (easier to confirm).

### Privilege Escalation Hypothesis Generation

When you have low-privilege shell access (SSH credentials, command injection shell), generate hypotheses for EACH privesc vector:
- **SUID binaries**: Hypothesis that a SUID binary (e.g., find, vim, python, nmap, bash) allows root command execution. Category: Boundary.
- **Sudo misconfig**: Hypothesis that `sudo -l` reveals NOPASSWD entries or exploitable commands (vim, find, env, awk, python). Category: Boundary.
- **Cron jobs**: Hypothesis that writable cron scripts or PATH hijackable cron jobs exist. Category: State.
- **Linux capabilities**: Hypothesis that binaries have dangerous capabilities (cap_setuid, cap_dac_override, cap_sys_admin). Category: Boundary.
- **PATH hijacking**: Hypothesis that a privileged script calls commands via relative path, exploitable by prepending a writable directory to PATH. Category: State.
- **Writable sensitive files**: Hypothesis that /etc/passwd, /etc/shadow, or systemd unit files are writable. Category: Confidentiality.
- **Kernel exploits**: Hypothesis that the kernel version is vulnerable to known exploits (DirtyPipe, DirtyCow, PwnKit). Category: Boundary.

Generate one hypothesis per vector. Each probe is a single targeted command (e.g., `find / -perm -4000` is a probe, NOT brute force). Prioritize SUID and sudo (most common CTF vectors) over kernel exploits (rare in labs).

### Network Services Hypothesis Generation

When network services (FTP, SSH, SMTP, SNMP, NFS, SMB, DNS, Telnet, etc.) are discovered, generate hypotheses for EACH service:
- **FTP anonymous access**: Hypothesis that FTP allows anonymous login (user=anonymous, pass=empty or email). Category: Boundary.
- **FTP writable upload**: Hypothesis that FTP allows file upload to web-accessible directories. Category: Input.
- **FTP version exploit**: Hypothesis that the FTP server version is vulnerable (e.g., vsftpd 2.3.4 backdoor, ProFTPD mod_copy). Category: Boundary.
- **SSH weak credentials**: Hypothesis that SSH accepts default or weak credentials (admin/admin, root/toor, user from FTP files). Category: Confidentiality.
- **SSH key reuse**: Hypothesis that SSH private keys found on other hosts/services grant access. Category: Confidentiality.
- **SMTP user enumeration**: Hypothesis that SMTP VRFY/EXPN/RCPT TO reveals valid usernames. Category: Confidentiality.
- **SNMP default community**: Hypothesis that SNMP uses default community strings (public, private). Category: Boundary.
- **NFS export exposure**: Hypothesis that NFS shares are mountable without auth, exposing sensitive files. Category: Confidentiality.
- **SMB null session**: Hypothesis that SMB allows null session enumeration of shares/users. Category: Boundary.
- **Service banner info leak**: Hypothesis that service banners reveal version info enabling targeted exploits. Category: Confidentiality.

**Cross-Service Chaining**: Credentials or files discovered on one service should immediately generate hypotheses for other services on the same or different hosts. E.g., username from FTP file + password from SMTP = try SSH.

**IMPORTANT — Cross-Session Intelligence**: When cross-session intelligence is available showing past patterns for these services (e.g., FTP anonymous access worked before, vsftpd was exploitable), PRIORITIZE those patterns first. Past success on similar services means higher confidence and lower cost — skip to the known-good technique before generating novel hypotheses.

Each hypothesis must: reference a specific component, state a testable claim, and have a category.
Generate 3-5 hypotheses per component, prioritized by expected yield.

**Exit L1 when**: You have at least 3 testable hypotheses with clear probe plans.

## L2 — Probing (3-5 Differential Probes per Hypothesis)

Test each hypothesis with targeted differential probes:
1. **Baseline probe**: Normal/expected request to establish baseline response (status, length, timing)
2. **Edge probe**: Boundary condition request (empty input, max length, special chars)
3. **Violation probe**: Request that SHOULD be rejected if the hypothesis is false (SQLi payload, path traversal, auth bypass)
4. Optional: **Timing probe** and **Encoding probe** for deeper analysis

### Injection-Specific Probe Strategies

**SQL Injection probing** (3 probes per parameter):
- Baseline: Normal value (e.g., `search=test`) — record status, length, timing
- Edge: Syntax-breaking value (e.g., `search=test'`) — look for SQL error messages, 500 status, or length change
- Violation: Tautology or UNION (e.g., `search=test' OR '1'='1`) — look for extra data, different row count, or changed behavior
- If error-based fails, try time-based: `search=test' AND SLEEP(3)--` — look for timing delta > 2x baseline

**Command Injection probing** (3 probes per parameter):
- Baseline: Normal value — record status, length, timing
- Edge: Benign separator (e.g., `param=value;`) — look for error vs normal handling
- Violation: Output-producing command (e.g., `param=value;id` or `param=value|id`) — look for command output in response body or length change
- If output-based fails, try time-based: `param=value;sleep 3` — look for timing delta > 2x baseline

**Privilege Escalation probing** (1 targeted command per vector):
- SUID: `find / -perm -4000 -type f 2>/dev/null` — look for exploitable binaries (find, vim, python, nmap, bash, env, cp, mv, awk, perl, ruby, php, node)
- Sudo: `sudo -l 2>/dev/null` — look for NOPASSWD entries, (ALL) rules, or exploitable commands
- Cron: `cat /etc/crontab; ls -la /etc/cron.d/ /etc/cron.daily/ 2>/dev/null; crontab -l 2>/dev/null` — look for writable scripts, relative paths, wildcard usage
- Capabilities: `getcap -r / 2>/dev/null` — look for cap_setuid, cap_dac_override, cap_sys_admin on binaries
- PATH: `echo $PATH; ls -la $(echo $PATH | tr ':' ' ') 2>/dev/null` — look for writable PATH directories (/tmp, ., user-writable dirs)

Each command is a single targeted probe — NOT brute force. Report the output as-is and flag anomalies (e.g., unusual SUID binary, NOPASSWD sudo, writable cron script).

**Network Services probing** (1-3 targeted probes per service):
- FTP anonymous: `ftp -n host <<< 'user anonymous\npass\nls\nquit'` or equivalent — look for successful login (230 response), directory listing, writable dirs
- FTP version: banner grab via `nc host 21` or nmap `-sV` — check version against known CVEs (vsftpd 2.3.4, ProFTPD 1.3.5 mod_copy)
- SSH weak creds: Try known credentials from KB (found on FTP, web, etc.) — NOT brute force, only credentials already discovered
- SMTP enum: `VRFY root`, `VRFY admin`, `EXPN postmaster` — look for 250 (exists) vs 550 (unknown)
- SNMP: `snmpwalk -v2c -c public host` — look for successful response vs timeout
- NFS: `showmount -e host` — look for exported shares
- SMB: `smbclient -N -L //host` — look for shares accessible without auth

Each network service probe is a single targeted connection — NOT a brute force dictionary attack. Cross-reference discoveries: usernames from SMTP enum become SSH candidates, files from FTP/NFS become credential sources.

Compare responses across probes. An anomaly (different status, significant length delta, timing difference, error message leak, command output in body) indicates the hypothesis may be confirmed.

Use `DifferentialProbe` tasks — each probe is a single targeted request. DO NOT exploit during probing.

**Exit L2 when**: All active hypotheses have been probed and classified as Confirmed or Refuted.

## L3 — Exploiting (Confirmed Only, Minimal PoC)

Exploit ONLY confirmed hypotheses (status = Confirmed). Never exploit unconfirmed guesses.
- Create minimal proof-of-concept for each confirmed vulnerability
- Extract proof: flags, credentials, sensitive data, shell access
- Use `ExploitHypothesis` tasks
- Stop exploiting once proof is obtained — do not over-exploit

### Injection Exploitation Strategies

**Confirmed SQL Injection**:
1. Determine injection type (error-based, UNION, blind boolean, blind time-based)
2. For UNION-based: find column count with ORDER BY, then extract data: database names → table names → column names → flag/credential data
3. For blind: use binary search with conditional queries to extract characters
4. Priority targets: user/password tables, configuration tables, flag tables, files (LOAD_FILE on MySQL, pg_read_file on PostgreSQL)

**Confirmed Command Injection**:
1. Determine working separator/syntax (`;`, `|`, `&&`, `$(...)`)
2. Run `id` and `whoami` to confirm execution context
3. Read flag files: `cat /flag*`, `find / -name 'flag*' -type f 2>/dev/null`
4. If limited output: use base64 encoding or write to web-accessible path
5. For privilege escalation: check `sudo -l`, SUID binaries, writable cron jobs

**Confirmed Privilege Escalation** (via SSH or shell):
1. **SUID binary**: Use GTFOBins-style exploitation — e.g., `find . -exec /bin/sh -p \\;`, `python -c 'import os;os.execl(\"/bin/sh\",\"sh\",\"-p\")'`, `vim -c ':!/bin/sh'`, `nmap --interactive` then `!sh`
2. **Sudo NOPASSWD**: Run the allowed command with shell escape — e.g., `sudo find . -exec /bin/sh \\;`, `sudo vim -c ':!/bin/sh'`, `sudo env /bin/sh`, `sudo awk 'BEGIN {{system(\"/bin/sh\")}}'`
3. **Writable cron job**: Replace the cron script content with: `cp /bin/bash /tmp/rootbash && chmod +s /tmp/rootbash` or `cat /root/flag* > /tmp/flag_out`, wait for execution
4. **Capabilities**: For cap_setuid: `python -c 'import os;os.setuid(0);os.system(\"/bin/sh\")'`; for cap_dac_override: read /etc/shadow or /root/flag directly
5. **PATH hijack**: Create malicious binary in writable PATH dir, trigger the privileged script
6. After escalation: `whoami` to confirm root, then `cat /root/flag*`, `find / -name 'flag*' 2>/dev/null`

**Confirmed Network Service Exploitation**:
1. **FTP anonymous + writable**: Upload a reverse shell or webshell to web-accessible dir; download all accessible files and search for flags/credentials
2. **FTP version exploit**: For vsftpd 2.3.4: connect to backdoor port 6200; for ProFTPD mod_copy: `SITE CPFR /etc/shadow` → `SITE CPTO /var/www/html/shadow.txt`
3. **SSH with found creds**: Login and enumerate: `id`, `whoami`, `cat /home/*/flag*`, `find / -name 'flag*' 2>/dev/null`; then pivot to privesc if needed
4. **SMTP user enum → SSH**: Use confirmed usernames with common passwords or credentials found elsewhere
5. **SNMP info leak**: Extract system info, running processes, network interfaces, installed software — use findings to generate new hypotheses
6. **NFS mount**: Mount the share locally, copy sensitive files (SSH keys, configs, flags), check for `.ssh/authorized_keys` writable to plant keys
7. **SMB access**: Download all accessible files, check for credentials in config files, scripts, or documents
8. **Cross-service chaining**: Credentials from FTP → try SSH; SSH keys from NFS → try SSH; usernames from SMTP → try FTP/SSH with found passwords

**Exit L3 when**: All confirmed hypotheses have been exploited or determined unexploitable.

## L4 — Chaining (Connect Findings)

Chain findings across components and hosts:
- Credentials from host A → try on host B, C, D
- File read on host A → read config files pointing to host B
- Shell on host A → pivot to internal network
- Information from one component → generate new hypotheses for another

After chaining, return to L1 with new hypotheses if new attack surface was discovered.

## Escalation Rule — Brute Force as Last Resort

Brute force is NOT forbidden, but it is the LAST resort. Use it only when:
1. ALL hypotheses for a target have been exhausted (confirmed+exploited or refuted)
2. No credentials exist in the KB for the target service
3. No other attack vectors remain viable
4. You provide explicit justification in the task rationale

When escalating to brute force, state: "All N hypotheses exhausted. Escalating to brute force because: [reason]."

# Priority Framework

```
PRIORITY = expected_yield * confidence / cost
```

Where:
- **expected_yield**: What you gain if successful (flag=high, creds=high, info=medium, enumeration=low)
- **confidence**: 0.0-1.0 estimate of success likelihood. Be calibrated: known creds on open port = 0.9+, confirmed hypothesis exploit = 0.8, unconfirmed probe = 0.4-0.6, brute force = 0.1-0.3
- **cost**: Number of requests/tool calls needed (probe=1-5 calls=low cost, enumeration=10-50=medium, brute force=100+=high cost)

Priority weights: Critical=100, High=75, Medium=50, Low=25, Background=10.
Effective score = priority_weight * confidence.

# Defender Awareness

When a Defender Model section is present in the situation report:
- **Check noise budget** before proposing actions. If `noise_budget < 1.0`, factor detection cost into priority: `adjusted_priority = base_priority * (1.0 - detection_cost * (1.0 - noise_budget))`.
- **Avoid blocked actions** — actions with detection cost > noise budget should use evasion techniques or be skipped.
- **Adaptive evasion** — when a payload is blocked (WAF detected), classify the WAF and select bypass techniques from the available list. Prefer techniques with higher success rates against the detected WAF type.
- **Rate limit awareness** — when rate limits are detected, slow down requests and batch probes to stay under limits.
- **Escalation path**: silent probes → low-detection actions → evasion-wrapped actions → noisy actions (only when noise budget allows).

Default detection costs (when no explicit costs set):
- Differential probes, fingerprinting: ~0.1 (very quiet)
- Port scans, credential tests: ~0.2
- Directory enumeration: ~0.3
- Injection exploits: ~0.5
- Brute force: ~0.8 (very noisy)

# Cognitive Heuristics

- **Use What You Have** — Credentials in KB > discovering new attack surface. Try existing creds on every accessible service first.
- **Go Inside Before Going Wider** — When access is gained, enumerate EVERYTHING inside before moving to the next target.
- **Reassess on Every Credential** — New credentials trigger a COMPLETE reassessment of all accessible services.
- **Connect the Dots** — Cross-reference findings. Username from host A + password from host B = try together.

# Anti-Patterns to AVOID

- **The Lawnmower**: Enumerating everything before any exploitation.
- **The Script Kiddie**: Running tools without understanding the target stack.
- **The Scatterbrain**: Starting everywhere, finishing nowhere.
- **The Brute**: Brute forcing when credentials exist in KB or hypotheses remain untested.
- **The Guesser**: Exploiting without confirming hypotheses first — always probe before exploit.

# Response Format

You MUST respond with a JSON object between `{marker}` markers, like this:

{marker}
{{
  "assessment": "Brief situation summary — what do we know, what's most promising, what's the attack strategy",
  "suggestions": [
    {{
      "action": "Scan for open ports on target",
      "commands": ["!nmap -sV -sC 10.10.10.1"],
      "rationale": "We have no service enumeration yet. Port scan is the first step in L0 Modeling.",
      "category": null,
      "expected_yield": "Service map, version info, potential entry points",
      "priority": "High",
      "confidence": 0.9
    }},
    {{
      "action": "Test for SQL injection on login form",
      "commands": ["!curl -d 'user=admin%27&pass=test' http://10.10.10.1/login"],
      "rationale": "Login form parameter 'user' may be vulnerable to SQLi based on error-based response.",
      "category": "Input",
      "expected_yield": "Confirmed SQLi → database access → flags/credentials",
      "priority": "Critical",
      "confidence": 0.7
    }}
  ],
  "memory_query": null,
  "hypotheses": [],
  "model_updates": [],
  "advance_layer": null
}}
{marker}

## Fields

- **assessment**: 2-4 sentence strategic summary of current situation and recommended approach.
- **suggestions**: Ordered list of recommended actions. Each has exact shell commands the user can run.
- **action**: Human-readable description of what to do.
- **commands**: Exact shell commands prefixed with `!` that the user can copy-paste.
- **rationale**: Why this action now — reference KB data and deductive reasoning.
- **category**: BISCL category if testing a hypothesis (Boundary/Input/State/Confidentiality/Logic), null otherwise.
- **expected_yield**: What we expect to learn or gain from this action.
- **priority**: One of "Critical", "High", "Medium", "Low", "Background".
- **confidence**: 0.0-1.0 float estimating likelihood of success. Be calibrated.
- **memory_query**: Optional. Request deeper cross-session memory lookup.
- **hypotheses**: List of hypotheses generated or updated in this cycle.
- **model_updates**: List of updates to apply to the system model.
- **advance_layer**: Optional. Set to advance the deductive layer.

Provide EXACT shell commands with correct flags. Be specific about tool options and expected output patterns.

Respond ONLY with the markers and JSON. No other text."#,
        marker = STRATEGY_MARKER,
    )
}

/// Format the system model into a markdown section for the strategist.
fn format_system_model(model: &SystemModel) -> String {
    let mut s = String::with_capacity(2048);
    s.push_str("# System Model\n\n");

    let layer_label = match model.current_layer {
        DeductiveLayer::Modeling => "L0 Modeling",
        DeductiveLayer::Hypothesizing => "L1 Hypothesizing",
        DeductiveLayer::Probing => "L2 Probing",
        DeductiveLayer::Exploiting => "L3 Exploiting",
        DeductiveLayer::Chaining => "L4 Chaining",
    };
    s.push_str(&format!(
        "- **Current layer**: {layer_label}\n- **Model confidence**: {:.2}\n\n",
        model.model_confidence
    ));

    if model.components.is_empty() {
        s.push_str("## Components\n\nNo components discovered yet.\n\n");
    } else {
        s.push_str(&format!(
            "## Components ({} discovered)\n\n",
            model.components.len()
        ));
        for c in &model.components {
            let ctype = match &c.component_type {
                ComponentType::WebApp => "WebApp",
                ComponentType::Database => "Database",
                ComponentType::AuthService => "AuthService",
                ComponentType::FileServer => "FileServer",
                ComponentType::MailServer => "MailServer",
                ComponentType::DnsServer => "DnsServer",
                ComponentType::CacheStore => "CacheStore",
                ComponentType::ContainerRuntime => "ContainerRuntime",
                ComponentType::Custom(name) => name.as_str(),
            };
            let port_str = c.port.map(|p| format!(":{p}")).unwrap_or_default();
            s.push_str(&format!(
                "### {} — {}{} [{}] (confidence={:.2})\n",
                c.id, c.host, port_str, ctype, c.confidence
            ));

            let mut stack_parts = Vec::new();
            if let Some(ref srv) = c.stack.server {
                stack_parts.push(format!("server={srv}"));
            }
            if let Some(ref fw) = c.stack.framework {
                stack_parts.push(format!("framework={fw}"));
            }
            if let Some(ref lang) = c.stack.language {
                stack_parts.push(format!("language={lang}"));
            }
            if !c.stack.technologies.is_empty() {
                stack_parts.push(format!("tech=[{}]", c.stack.technologies.join(", ")));
            }
            if !stack_parts.is_empty() {
                s.push_str(&format!("- Stack: {}\n", stack_parts.join(", ")));
            }

            if !c.entry_points.is_empty() {
                s.push_str(&format!("- Entry points: {}\n", c.entry_points.len()));
                for ep in c.entry_points.iter().take(10) {
                    let auth = if ep.auth_required { " [auth]" } else { "" };
                    s.push_str(&format!("  - {} {}{}\n", ep.method, ep.path, auth));
                }
                if c.entry_points.len() > 10 {
                    s.push_str(&format!("  - ... and {} more\n", c.entry_points.len() - 10));
                }
            }
            s.push('\n');
        }
    }

    if !model.trust_boundaries.is_empty() {
        s.push_str(&format!(
            "## Trust Boundaries ({})\n\n",
            model.trust_boundaries.len()
        ));
        for b in &model.trust_boundaries {
            s.push_str(&format!(
                "- **{}** ({}): [{}]\n",
                b.name,
                b.id,
                b.components.join(", ")
            ));
        }
        s.push('\n');
    }

    if model.hypotheses.is_empty() {
        s.push_str("## Hypotheses\n\nNo hypotheses yet.\n\n");
    } else {
        let proposed = model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Proposed)
            .count();
        let probing = model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Probing)
            .count();
        let confirmed = model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Confirmed)
            .count();
        let refuted = model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Refuted)
            .count();
        let exploited = model
            .hypotheses
            .iter()
            .filter(|h| h.status == HypothesisStatus::Exploited)
            .count();

        s.push_str(&format!(
            "## Hypotheses ({} total: {} proposed, {} probing, {} confirmed, {} refuted, {} exploited)\n\n",
            model.hypotheses.len(), proposed, probing, confirmed, refuted, exploited
        ));
        for h in &model.hypotheses {
            let status = match h.status {
                HypothesisStatus::Proposed => "PROPOSED",
                HypothesisStatus::Probing => "PROBING",
                HypothesisStatus::Confirmed => "CONFIRMED",
                HypothesisStatus::Refuted => "REFUTED",
                HypothesisStatus::Exploited => "EXPLOITED",
            };
            let cat = match h.category {
                crate::agent::knowledge::HypothesisCategory::Boundary => "Boundary",
                crate::agent::knowledge::HypothesisCategory::Input => "Input",
                crate::agent::knowledge::HypothesisCategory::State => "State",
                crate::agent::knowledge::HypothesisCategory::Confidentiality => "Confidentiality",
                crate::agent::knowledge::HypothesisCategory::Logic => "Logic",
            };
            s.push_str(&format!(
                "- [{}] **{}** (component={}, category={}, confidence={:.2}, probes={})\n  {}\n",
                status,
                h.id,
                h.component_id,
                cat,
                h.confidence,
                h.probes.len(),
                h.statement
            ));
        }
        s.push('\n');
    }

    s
}

/// Format cross-session intelligence into a markdown section (~1500 token cap).
fn format_cross_session_intel(intel: &CrossSessionIntel) -> String {
    let mut section = String::with_capacity(2048);
    section.push_str("# Cross-Session Intelligence\n\n");

    if !intel.relevant_patterns.is_empty() {
        section.push_str("## Known Attack Patterns\n\n");
        for p in intel.relevant_patterns.iter().take(5) {
            let success_rate = if p.total_attempts > 0 {
                (p.successes as f64 / p.total_attempts as f64) * 100.0
            } else {
                0.0
            };
            section.push_str(&format!(
                "- **{}** on {}/{}: {:.0}% success ({}/{} attempts), avg {:.0} tool calls, {:.0}s{}\n",
                p.technique,
                p.service_type,
                p.technology_stack,
                success_rate,
                p.successes,
                p.total_attempts,
                p.avg_tool_calls,
                p.avg_duration_secs,
                if p.brute_force_needed { " [brute force needed]" } else { "" },
            ));
        }
        section.push('\n');
    }

    if !intel.technique_stats.is_empty() {
        section.push_str("## Technique Cost Estimates\n\n");
        for t in intel.technique_stats.iter().take(5) {
            section.push_str(&format!(
                "- **{}**: {:.0}% success rate, avg {:.1} tool calls, avg {:.0}s\n",
                t.task_type,
                t.success_rate * 100.0,
                t.avg_tool_calls,
                t.avg_duration,
            ));
        }
        section.push('\n');
    }

    if !intel.similar_sessions.is_empty() {
        section.push_str("## Similar Past Sessions\n\n");
        for s in intel.similar_sessions.iter().take(3) {
            section.push_str(&format!(
                "- **{}** ({}): {} hosts, {} flags — {}\n  Services: {} | Technologies: {}\n",
                s.session_id,
                s.outcome,
                s.hosts_count,
                s.flags_captured,
                s.summary,
                s.services_seen,
                s.technologies_seen,
            ));
        }
        section.push('\n');
    }

    section
}

/// Format the defender model into a markdown section for the strategist.
fn format_defender_model(defender: &crate::agent::knowledge::DefenderModel) -> String {
    use crate::agent::knowledge::IdsSensitivity;

    let has_defenses = !defender.detected_wafs.is_empty()
        || defender.ids_sensitivity != IdsSensitivity::None
        || !defender.rate_limits.is_empty()
        || !defender.blocked_payloads.is_empty();

    if !has_defenses && defender.noise_budget >= 1.0 {
        return String::new();
    }

    let mut s = String::with_capacity(1024);
    s.push_str("# Defender Model\n\n");
    s.push_str(&format!(
        "- **Noise budget**: {:.2} (0.0=silent, 1.0=unconstrained)\n",
        defender.noise_budget
    ));

    let ids_label = match defender.ids_sensitivity {
        IdsSensitivity::None => "None",
        IdsSensitivity::Low => "Low",
        IdsSensitivity::Medium => "Medium",
        IdsSensitivity::High => "High",
    };
    s.push_str(&format!("- **IDS sensitivity**: {ids_label}\n\n"));

    if !defender.detected_wafs.is_empty() {
        s.push_str("## Detected WAFs\n\n");
        for waf in &defender.detected_wafs {
            let waf_label = match &waf.waf_type {
                crate::agent::knowledge::WafType::ModSecurity => "ModSecurity",
                crate::agent::knowledge::WafType::Cloudflare => "Cloudflare",
                crate::agent::knowledge::WafType::AwsWaf => "AWS WAF",
                crate::agent::knowledge::WafType::Imperva => "Imperva",
                crate::agent::knowledge::WafType::F5BigIp => "F5 BIG-IP",
                crate::agent::knowledge::WafType::Akamai => "Akamai",
                crate::agent::knowledge::WafType::Unknown(s) => s.as_str(),
            };
            let port_str = waf.port.map(|p| format!(":{p}")).unwrap_or_default();
            s.push_str(&format!(
                "- **{}{}**: {} (confidence={:.2}, {} blocked, {} bypasses)\n",
                waf.host,
                port_str,
                waf_label,
                waf.confidence,
                waf.blocked_payloads.len(),
                waf.successful_bypasses.len(),
            ));
            if !waf.successful_bypasses.is_empty() {
                s.push_str(&format!(
                    "  Known bypasses: {}\n",
                    waf.successful_bypasses.join(", ")
                ));
            }
        }
        s.push('\n');
    }

    if !defender.rate_limits.is_empty() {
        s.push_str("## Rate Limits\n\n");
        for rl in &defender.rate_limits {
            let ep = rl.endpoint.as_deref().unwrap_or("*");
            s.push_str(&format!(
                "- {}/{}: {}/{}s (HTTP {})\n",
                rl.host, ep, rl.max_requests, rl.window_secs, rl.limit_status
            ));
        }
        s.push('\n');
    }

    let risky_actions: Vec<_> = defender
        .action_costs
        .iter()
        .filter(|c| c.cost > defender.noise_budget)
        .collect();
    if !risky_actions.is_empty() {
        s.push_str("## Actions EXCEEDING Noise Budget\n\n");
        for a in &risky_actions {
            s.push_str(&format!(
                "- **{}**: cost={:.2} > budget={:.2} — {}\n",
                a.action, a.cost, defender.noise_budget, a.rationale
            ));
        }
        s.push_str("\nUse evasion techniques or avoid these actions.\n\n");
    }

    if !defender.bypass_techniques.is_empty() {
        s.push_str("## Available Bypass Techniques\n\n");
        for t in &defender.bypass_techniques {
            s.push_str(&format!(
                "- **{}**: {} (success_rate={:.0}%)\n",
                t.name,
                t.description,
                t.success_rate * 100.0,
            ));
        }
        s.push('\n');
    }

    s.push_str("**Priority adjustment**: When noise budget < 1.0, factor detection cost into hypothesis prioritization: `adjusted_priority = base_priority * (1.0 - detection_cost * (1.0 - noise_budget))`. Prefer low-detection actions.\n\n");

    s
}

fn build_user_message(kb: &KnowledgeBase, intel: Option<&CrossSessionIntel>) -> String {
    let mut msg = String::with_capacity(8192);

    // Section 0: Session Objective
    {
        let goal = &kb.goal;
        let goal_type_label = match &goal.goal_type {
            crate::agent::knowledge::GoalType::CaptureFlags { .. } => "CaptureFlags",
            crate::agent::knowledge::GoalType::GainAccess { .. } => "GainAccess",
            crate::agent::knowledge::GoalType::Exfiltrate { .. } => "Exfiltrate",
            crate::agent::knowledge::GoalType::VulnerabilityAssessment { .. } => {
                "VulnerabilityAssessment"
            }
            crate::agent::knowledge::GoalType::Custom { .. } => "Custom",
        };
        let status_label = match &goal.status {
            GoalStatus::InProgress => "In Progress",
            GoalStatus::Achieved => "ACHIEVED",
            GoalStatus::PartiallyAchieved => "Partially Achieved",
            GoalStatus::Failed => "Failed",
        };
        let total = goal.success_criteria.len();
        let met = goal.success_criteria.iter().filter(|c| c.met).count();

        msg.push_str("# Session Objective\n\n");
        msg.push_str(&format!("- **Goal type**: {goal_type_label}\n"));
        msg.push_str(&format!("- **Description**: {}\n", goal.description));
        msg.push_str(&format!("- **Overall status**: {status_label}\n"));
        msg.push_str(&format!("- **Progress**: {met}/{total} criteria met\n\n"));

        if !goal.success_criteria.is_empty() {
            msg.push_str("**Criteria:**\n");
            for criterion in &goal.success_criteria {
                let mark = if criterion.met { "MET" } else { "UNMET" };
                msg.push_str(&format!("- [{}] {}\n", mark, criterion.description));
            }
            msg.push('\n');
        }
    }

    // Section 1: Situation report from KB
    msg.push_str("# Current Situation Report\n\n");
    msg.push_str(&kb.situation_report());
    msg.push_str("\n\n");

    // Section 2: System Model (deductive state)
    msg.push_str(&format_system_model(&kb.system_model));

    // Section 3: Cross-Session Intelligence (if available)
    if let Some(intel) = intel {
        msg.push_str(&format_cross_session_intel(intel));
    }

    // Section 4: Defender Model (WAFs, IDS, rate limits, noise budget)
    {
        let defender_section = format_defender_model(&kb.defender_model);
        if !defender_section.is_empty() {
            msg.push_str(&defender_section);
        }
    }

    msg.push_str("Analyse the situation and suggest the next actions for the operator.\n");
    msg
}

// ---------------------------------------------------------------------------
// Response parsing
// ---------------------------------------------------------------------------

pub fn parse_advisor_response(output: &str) -> Result<AdvisorSuggestion, String> {
    let parts: Vec<&str> = output.split(STRATEGY_MARKER).collect();

    if parts.len() < 3 {
        return Err(format!(
            "expected ===REDTRAIL_STRATEGY=== markers (found {} parts, need 3+)",
            parts.len()
        ));
    }

    let json_str = parts[1].trim();

    serde_json::from_str::<AdvisorSuggestion>(json_str).map_err(|e| {
        let preview: String = json_str.chars().take(200).collect();
        format!("JSON parse error: {e} | preview: {preview}")
    })
}

/// Legacy parse function — kept for backward compatibility with existing tests.
pub fn parse_strategy_response(output: &str) -> Result<StrategistPlan, String> {
    let parts: Vec<&str> = output.split(STRATEGY_MARKER).collect();

    if parts.len() < 3 {
        return Err(format!(
            "expected ===REDTRAIL_STRATEGY=== markers (found {} parts, need 3+)",
            parts.len()
        ));
    }

    let json_str = parts[1].trim();

    serde_json::from_str::<StrategistPlan>(json_str).map_err(|e| {
        let preview: String = json_str.chars().take(200).collect();
        format!("JSON parse error: {e} | preview: {preview}")
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prompt_contains_kb_situation_report() {
        let kb = KnowledgeBase::new();
        let msg = build_user_message(&kb, None);

        assert!(msg.contains("# Session Objective"));
        assert!(msg.contains("# Current Situation Report"));
        assert!(msg.contains("# System Model"));
    }

    #[test]
    fn test_system_model_section_empty() {
        let kb = KnowledgeBase::new();
        let msg = build_user_message(&kb, None);

        assert!(msg.contains("# System Model"));
        assert!(msg.contains("**Current layer**: L0 Modeling"));
        assert!(msg.contains("No components discovered yet"));
        assert!(msg.contains("No hypotheses yet"));
    }

    #[test]
    fn test_system_model_section_with_components_and_hypotheses() {
        use crate::agent::knowledge::{
            ComponentType, DeductiveLayer, Hypothesis, HypothesisCategory, HypothesisStatus,
            StackFingerprint, SystemComponent, TrustBoundary,
        };

        let mut kb = KnowledgeBase::new();
        kb.system_model.current_layer = DeductiveLayer::Probing;
        kb.system_model.model_confidence = 0.65;
        kb.system_model.components.push(SystemComponent {
            id: "web1".into(),
            host: "10.0.0.1".into(),
            port: Some(80),
            component_type: ComponentType::WebApp,
            stack: StackFingerprint {
                server: Some("nginx".into()),
                framework: Some("Flask".into()),
                language: Some("Python".into()),
                technologies: vec!["SQLite".into()],
            },
            entry_points: vec![],
            confidence: 0.8,
        });
        kb.system_model.trust_boundaries.push(TrustBoundary {
            id: "tb1".into(),
            name: "DMZ".into(),
            components: vec!["web1".into()],
        });
        kb.system_model.hypotheses.push(Hypothesis {
            id: "h1".into(),
            component_id: "web1".into(),
            category: HypothesisCategory::Input,
            statement: "Login form vulnerable to SQLi".into(),
            status: HypothesisStatus::Probing,
            probes: vec![],
            confidence: 0.7,
            task_ids: vec![],
        });

        let msg = build_user_message(&kb, None);

        assert!(msg.contains("**Current layer**: L2 Probing"));
        assert!(msg.contains("**Model confidence**: 0.65"));
        assert!(msg.contains("Components (1 discovered)"));
        assert!(msg.contains("web1 — 10.0.0.1:80 [WebApp]"));
        assert!(msg.contains("server=nginx"));
        assert!(msg.contains("framework=Flask"));
        assert!(msg.contains("Trust Boundaries (1)"));
        assert!(msg.contains("DMZ"));
        assert!(msg.contains("Hypotheses (1 total"));
        assert!(msg.contains("[PROBING] **h1**"));
        assert!(msg.contains("Login form vulnerable to SQLi"));
    }

    #[test]
    fn test_prompt_contains_deductive_protocol() {
        let prompt = build_system_prompt();

        assert!(prompt.contains("L0 — Modeling"));
        assert!(prompt.contains("L1 — Hypothesizing"));
        assert!(prompt.contains("BISCL"));
        assert!(prompt.contains("L2 — Probing"));
        assert!(prompt.contains("L3 — Exploiting"));
        assert!(prompt.contains("L4 — Chaining"));
        assert!(prompt.contains("Brute Force as Last Resort"));
        assert!(prompt.contains("ALL hypotheses"));
        assert!(prompt.contains("NOT forbidden"));
        assert!(prompt.contains("expected_yield * confidence / cost"));
        assert!(prompt.contains("Use What You Have"));
        assert!(prompt.contains("Reassess on Every Credential"));
    }

    #[test]
    fn test_parse_valid_strategy_response() {
        let response = format!(
            r#"Some preamble text
{marker}
{{
  "assessment": "Initial recon shows 3 hosts with web services. Priority is credential discovery.",
  "tasks": [
    {{
      "definition_name": "WebEnum",
      "params": {{"url": "http://10.0.0.1"}},
      "priority": "High",
      "rationale": "Web service on port 80, need to enumerate before attacking",
      "confidence": 0.8
    }},
    {{
      "definition_name": "PortScan",
      "params": {{"host": "10.0.0.2"}},
      "priority": "Medium",
      "rationale": "New host discovered, need service enumeration",
      "confidence": 0.7
    }}
  ],
  "new_definitions": [],
  "is_complete": false
}}
{marker}
Some trailing text"#,
            marker = STRATEGY_MARKER,
        );

        let plan = parse_strategy_response(&response).expect("should parse successfully");

        assert_eq!(plan.tasks.len(), 2);
        assert!(!plan.is_complete);
        assert!(plan.assessment.contains("Initial recon"));
        assert_eq!(plan.tasks[0].definition_name, "WebEnum");
        assert_eq!(plan.tasks[0].priority, Priority::High);
        assert!((plan.tasks[0].confidence - 0.8).abs() < f32::EPSILON);
        assert_eq!(plan.tasks[1].definition_name, "PortScan");
        assert_eq!(plan.tasks[1].priority, Priority::Medium);
    }

    #[test]
    fn test_parse_strategy_is_complete() {
        let response = format!(
            r#"{marker}
{{
  "assessment": "All flags captured. Assessment complete.",
  "tasks": [],
  "new_definitions": [],
  "is_complete": true
}}
{marker}"#,
            marker = STRATEGY_MARKER,
        );

        let plan = parse_strategy_response(&response).expect("should parse");
        assert!(plan.is_complete);
        assert!(plan.tasks.is_empty());
    }

    #[test]
    fn test_parse_strategy_no_markers() {
        let response = "Just some random LLM output without markers";
        let result = parse_strategy_response(response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("markers"));
    }

    #[test]
    fn test_parse_strategy_invalid_json() {
        let response = format!(
            "{marker}\nnot valid json\n{marker}",
            marker = STRATEGY_MARKER,
        );
        let result = parse_strategy_response(&response);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON parse error"));
    }

    #[test]
    fn test_proposed_task_effective_score() {
        let task = ProposedTask {
            definition_name: "PortScan".into(),
            params: HashMap::new(),
            priority: Priority::High,
            rationale: "test".into(),
            confidence: 0.8,
        };
        assert!((task.effective_score() - 60.0).abs() < f32::EPSILON);
    }

    #[test]
    fn test_effective_score_all_priorities() {
        let cases = vec![
            (Priority::Critical, 1.0, 100.0),
            (Priority::High, 1.0, 75.0),
            (Priority::Medium, 1.0, 50.0),
            (Priority::Low, 1.0, 25.0),
            (Priority::Background, 1.0, 10.0),
            (Priority::Critical, 0.5, 50.0),
            (Priority::High, 0.0, 0.0),
        ];
        for (priority, confidence, expected) in cases {
            let task = ProposedTask {
                definition_name: "Test".into(),
                params: HashMap::new(),
                priority,
                rationale: String::new(),
                confidence,
            };
            assert!(
                (task.effective_score() - expected).abs() < f32::EPSILON,
                "priority={:?} confidence={} => expected {} got {}",
                priority,
                confidence,
                expected,
                task.effective_score()
            );
        }
    }

    #[test]
    fn test_system_prompt_contains_response_format() {
        let prompt = build_system_prompt();
        assert!(prompt.contains(STRATEGY_MARKER));
        assert!(prompt.contains("assessment"));
        assert!(prompt.contains("suggestions"));
        assert!(prompt.contains("confidence"));
        assert!(prompt.contains("action"));
        assert!(prompt.contains("commands"));
        assert!(prompt.contains("rationale"));
    }

    #[test]
    fn test_parse_strategy_with_cancel() {
        let response = format!(
            r#"{marker}
{{
  "assessment": "Too many redundant TryCredentials tasks. Cancelling them.",
  "cancel": [
    {{
      "definition_name": "TryCredentials",
      "host": null,
      "reason": "redundant credential spray — all already tried"
    }},
    {{
      "definition_name": "BruteForce",
      "host": "10.0.0.5",
      "reason": "we already have creds for this host"
    }}
  ],
  "tasks": [],
  "new_definitions": [],
  "is_complete": false
}}
{marker}"#,
            marker = STRATEGY_MARKER,
        );

        let plan = parse_strategy_response(&response).expect("should parse");
        assert_eq!(plan.cancel.len(), 2);
        assert_eq!(
            plan.cancel[0].definition_name.as_deref(),
            Some("TryCredentials")
        );
        assert!(plan.cancel[0].host.is_none());
        assert_eq!(
            plan.cancel[1].definition_name.as_deref(),
            Some("BruteForce")
        );
        assert_eq!(plan.cancel[1].host.as_deref(), Some("10.0.0.5"));
    }

    #[test]
    fn test_prompt_includes_session_objective_with_criteria() {
        use crate::agent::knowledge::{Criterion, CriterionCheck, GoalType, SessionGoal};

        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: "FLAG{.*}".into(),
                expected_count: Some(3),
            },
            description: "Capture all 3 flags".into(),
            success_criteria: vec![
                Criterion {
                    description: "Capture at least 3 flags".into(),
                    check: CriterionCheck::FlagsCaptured { min_count: 3 },
                    met: true,
                },
                Criterion {
                    description: "Gain root on 10.0.0.1".into(),
                    check: CriterionCheck::AccessObtained {
                        host: "10.0.0.1".into(),
                        min_privilege: "high".into(),
                    },
                    met: false,
                },
            ],
            status: GoalStatus::PartiallyAchieved,
        };

        let msg = build_user_message(&kb, None);

        assert!(msg.contains("# Session Objective"));
        assert!(msg.contains("**Goal type**: CaptureFlags"));
        assert!(msg.contains("**Description**: Capture all 3 flags"));
        assert!(msg.contains("**Overall status**: Partially Achieved"));
        assert!(msg.contains("**Progress**: 1/2 criteria met"));
        assert!(msg.contains("[MET] Capture at least 3 flags"));
        assert!(msg.contains("[UNMET] Gain root on 10.0.0.1"));
    }

    #[test]
    fn test_build_relevance_query_empty_kb() {
        let kb = KnowledgeBase::new();
        assert!(build_relevance_query(&kb).is_none());
    }

    #[test]
    fn test_build_relevance_query_with_hosts() {
        use crate::agent::knowledge::HostInfo;

        let mut kb = KnowledgeBase::new();
        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.1".into(),
            ports: vec![22, 80],
            services: vec!["ssh".into(), "http".into()],
            os: None,
        });
        kb.discovered_hosts.push(HostInfo {
            ip: "10.0.0.2".into(),
            ports: vec![80],
            services: vec!["http".into()],
            os: None,
        });

        let query = build_relevance_query(&kb).expect("should produce query");
        assert_eq!(query.services, vec!["http", "ssh"]);
        assert!(query.technologies.is_empty());
        assert_eq!(query.goal_type, Some("Custom".into()));
    }

    #[test]
    fn test_format_cross_session_intel_section() {
        use crate::db::{AttackPattern, CrossSessionIntel, SessionFingerprint, TechniqueStats};

        let intel = CrossSessionIntel {
            relevant_patterns: vec![AttackPattern {
                id: 1,
                technique: "SQLi".into(),
                vulnerability_class: "injection".into(),
                service_type: "http".into(),
                technology_stack: "php".into(),
                total_attempts: 10,
                successes: 8,
                avg_tool_calls: 5.0,
                avg_duration_secs: 30.0,
                brute_force_needed: false,
                attack_chain: "".into(),
                first_seen_at: "".into(),
                last_seen_at: "".into(),
                last_session_id: "".into(),
            }],
            similar_sessions: vec![SessionFingerprint {
                session_id: "sess-001".into(),
                services_seen: "http,ssh".into(),
                technologies_seen: "php,mysql".into(),
                vuln_classes_found: "injection".into(),
                flags_captured: 3,
                hosts_count: 2,
                summary: "Web app pentest".into(),
                outcome: "Achieved".into(),
                goal_type: "CaptureFlags".into(),
            }],
            technique_stats: vec![TechniqueStats {
                task_type: "WebEnum".into(),
                avg_tool_calls: 4.5,
                success_rate: 0.85,
                avg_duration: 25.0,
            }],
        };

        let section = format_cross_session_intel(&intel);
        assert!(section.contains("# Cross-Session Intelligence"));
        assert!(section.contains("## Known Attack Patterns"));
        assert!(section.contains("**SQLi** on http/php: 80% success"));
        assert!(section.contains("## Technique Cost Estimates"));
        assert!(section.contains("**WebEnum**: 85% success rate"));
        assert!(section.contains("## Similar Past Sessions"));
        assert!(section.contains("sess-001"));
        assert!(section.contains("Web app pentest"));
    }

    #[test]
    fn test_memory_query_serde_roundtrip() {
        let mq = MemoryQuery {
            query_type: "ftp_exploits".into(),
            services: vec!["ftp".into(), "ssh".into()],
            technologies: vec!["vsftpd".into()],
            tags: vec!["ctf".into()],
        };
        let json = serde_json::to_string(&mq).unwrap();
        let parsed: MemoryQuery = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.query_type, "ftp_exploits");
        assert_eq!(parsed.services, vec!["ftp", "ssh"]);
        assert_eq!(parsed.technologies, vec!["vsftpd"]);
        assert_eq!(parsed.tags, vec!["ctf"]);
    }

    #[test]
    fn test_parse_strategy_with_memory_query() {
        let response = format!(
            r#"{marker}
{{
  "assessment": "Unfamiliar FTP service. Querying memory for past FTP experience.",
  "tasks": [],
  "new_definitions": [],
  "is_complete": false,
  "memory_query": {{
    "query_type": "ftp_exploits",
    "services": ["ftp"],
    "technologies": ["vsftpd"],
    "tags": ["ctf"]
  }}
}}
{marker}"#,
            marker = STRATEGY_MARKER,
        );

        let plan = parse_strategy_response(&response).expect("should parse");
        assert!(plan.memory_query.is_some());
        let mq = plan.memory_query.unwrap();
        assert_eq!(mq.query_type, "ftp_exploits");
        assert_eq!(mq.services, vec!["ftp"]);
        assert_eq!(mq.technologies, vec!["vsftpd"]);
        assert_eq!(mq.tags, vec!["ctf"]);
    }

    #[test]
    fn test_system_prompt_contains_defender_awareness() {
        let prompt = build_system_prompt();
        assert!(prompt.contains("Defender Awareness"));
        assert!(prompt.contains("noise budget"));
        assert!(prompt.contains("detection cost"));
        assert!(prompt.contains("Adaptive evasion"));
        assert!(prompt.contains("adjusted_priority"));
    }

    #[test]
    fn test_defender_model_section_hidden_when_no_defenses() {
        let kb = KnowledgeBase::new();
        let msg = build_user_message(&kb, None);
        assert!(
            !msg.contains("# Defender Model"),
            "Defender Model section should be hidden when no defenses detected"
        );
    }

    #[test]
    fn test_defender_model_section_shown_when_waf_detected() {
        use crate::agent::knowledge::{DetectedWaf, WafType};

        let mut kb = KnowledgeBase::new();
        kb.defender_model.detected_wafs.push(DetectedWaf {
            host: "10.0.0.1".into(),
            port: Some(80),
            waf_type: WafType::ModSecurity,
            confidence: 0.7,
            blocked_payloads: vec!["' OR 1=1--".into()],
            successful_bypasses: vec!["case_alternation".into()],
        });
        kb.defender_model.noise_budget = 0.6;

        let msg = build_user_message(&kb, None);
        assert!(msg.contains("# Defender Model"));
        assert!(msg.contains("Noise budget"));
        assert!(msg.contains("0.60"));
        assert!(msg.contains("ModSecurity"));
        assert!(msg.contains("10.0.0.1"));
        assert!(msg.contains("case_alternation"));
    }

    #[test]
    fn test_defender_model_section_shows_rate_limits() {
        use crate::agent::knowledge::RateLimit;

        let mut kb = KnowledgeBase::new();
        kb.defender_model.rate_limits.push(RateLimit {
            host: "10.0.0.1".into(),
            endpoint: Some("/api/login".into()),
            max_requests: 5,
            window_secs: 60,
            limit_status: 429,
        });
        kb.defender_model.noise_budget = 0.8;

        let msg = build_user_message(&kb, None);
        assert!(msg.contains("Rate Limits"));
        assert!(msg.contains("/api/login"));
        assert!(msg.contains("5/60s"));
    }

    #[test]
    fn test_defender_model_section_shows_bypass_techniques() {
        let mut kb = KnowledgeBase::new();
        kb.defender_model = crate::agent::knowledge::DefenderModel::with_default_bypasses();
        kb.defender_model.noise_budget = 0.5;
        kb.defender_model.record_block("10.0.0.1", "payload", 403);

        let msg = build_user_message(&kb, None);
        assert!(msg.contains("Available Bypass Techniques"));
        assert!(msg.contains("case_alternation"));
        assert!(msg.contains("Priority adjustment"));
    }

    #[test]
    fn test_system_prompt_contains_network_services_guidance() {
        let prompt = build_system_prompt();

        assert!(prompt.contains("Network Services Hypothesis Generation"));
        assert!(prompt.contains("FTP anonymous access"));
        assert!(prompt.contains("SSH weak credentials"));
        assert!(prompt.contains("SMTP user enumeration"));
        assert!(prompt.contains("Cross-Session Intelligence"));
        assert!(prompt.contains("Network Services probing"));
        assert!(prompt.contains("Confirmed Network Service Exploitation"));
        assert!(prompt.contains("Cross-service chaining"));
    }

    #[test]
    fn test_cross_session_memory_flow() {
        use crate::agent::knowledge::{
            ComponentType, GoalType, HostInfo, SessionGoal, StackFingerprint, SystemComponent,
        };
        use crate::db::{AttackPattern, SessionFingerprint, TechniqueExecution, Db};

        let db = Db::open_in_memory().unwrap();

        let exec = TechniqueExecution {
            id: 0,
            session_id: "module01-session".into(),
            task_type: "ServiceEnum".into(),
            target_host: "172.20.1.10".into(),
            target_service: "ftp".into(),
            tool_calls: 3,
            wall_clock_secs: 15.0,
            succeeded: true,
            brute_force_used: false,
            technology_stack: "vsftpd".into(),
            executed_at: "2026-03-09T10:00:00Z".into(),
        };
        db.record_execution(&exec).unwrap();

        let pattern = AttackPattern {
            id: 0,
            technique: "ftp_anonymous".into(),
            vulnerability_class: "auth_bypass".into(),
            service_type: "ftp".into(),
            technology_stack: "vsftpd".into(),
            total_attempts: 1,
            successes: 1,
            avg_tool_calls: 2.0,
            avg_duration_secs: 10.0,
            brute_force_needed: false,
            attack_chain: r#"["ServiceEnum","FtpAnonymous"]"#.into(),
            first_seen_at: "2026-03-09T10:00:00Z".into(),
            last_seen_at: "2026-03-09T10:05:00Z".into(),
            last_session_id: "module01-session".into(),
        };
        db.upsert_attack_pattern(&pattern).unwrap();

        let fp = SessionFingerprint {
            session_id: "module01-session".into(),
            services_seen: "ftp,ssh,http".into(),
            technologies_seen: "vsftpd,openssh,apache".into(),
            vuln_classes_found: "auth_bypass,weak_credentials".into(),
            flags_captured: 4,
            hosts_count: 3,
            summary: "3 hosts, 4 flags, FTP anon access".into(),
            outcome: "achieved".into(),
            goal_type: "CaptureFlags".into(),
        };
        db.save_fingerprint(&fp).unwrap();

        let mut kb = KnowledgeBase::new();
        kb.goal = SessionGoal {
            goal_type: GoalType::CaptureFlags {
                flag_pattern: "FLAG\\{[^}]+\\}".into(),
                expected_count: Some(4),
            },
            description: "Capture all flags".into(),
            success_criteria: vec![],
            status: crate::agent::knowledge::GoalStatus::InProgress,
        };
        kb.discovered_hosts.push(HostInfo {
            ip: "172.20.7.10".into(),
            ports: vec![21, 22, 80],
            services: vec!["ftp".into(), "ssh".into(), "http".into()],
            os: None,
        });
        kb.system_model.components.push(SystemComponent {
            id: "ftp-srv".into(),
            host: "172.20.7.10".into(),
            port: Some(21),
            component_type: ComponentType::FileServer,
            stack: StackFingerprint {
                server: Some("vsftpd".into()),
                framework: None,
                language: None,
                technologies: vec![],
            },
            entry_points: vec![],
            confidence: 0.6,
        });

        let query = build_relevance_query(&kb).expect("should produce query");
        assert!(query.services.contains(&"ftp".to_string()));

        let intel = query_cross_session_intel(&kb, &db).expect("should find intel");
        assert!(!intel.relevant_patterns.is_empty());

        let msg = build_user_message(&kb, Some(&intel));
        assert!(msg.contains("# Cross-Session Intelligence"));
        assert!(msg.contains("ftp_anonymous"));
    }
}
