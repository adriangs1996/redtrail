use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Node types in the attack graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AttackNode {
    Host {
        ip: String,
        os: Option<String>,
        confidence: f32,
        last_verified: u64,
    },
    Service {
        name: String,
        port: u16,
        version: Option<String>,
        confidence: f32,
        last_verified: u64,
    },
    Credential {
        username: String,
        credential_type: CredentialType,
        confidence: f32,
        last_verified: u64,
    },
    Vulnerability {
        id: String,
        vuln_class: String,
        severity: String,
        description: String,
        confidence: f32,
        last_verified: u64,
    },
    AccessLevel {
        user: String,
        privilege: String,
        confidence: f32,
        last_verified: u64,
    },
    Hypothesis {
        id: String,
        statement: String,
        category: String,
        status: String,
        confidence: f32,
        last_verified: u64,
    },
}

/// Type of credential stored in a Credential node.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum CredentialType {
    Password,
    Hash,
    Key,
    Token,
}

/// Edge types representing relationships between nodes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AttackEdge {
    /// Host RUNS Service
    Runs,
    /// Service/Host VULNERABLE_TO Vulnerability
    VulnerableTo,
    /// Credential/Vulnerability GRANTS_ACCESS_TO AccessLevel
    GrantsAccessTo,
    /// Host ROUTES_TO Host (network connectivity)
    RoutesTo { protocol: String },
    /// ProbeResult DISPROVES Hypothesis
    Disproves,
    /// ProbeResult/Vulnerability CONFIRMS Hypothesis
    Confirms,
}

impl AttackNode {
    /// Get the confidence score for this node.
    pub fn confidence(&self) -> f32 {
        match self {
            AttackNode::Host { confidence, .. }
            | AttackNode::Service { confidence, .. }
            | AttackNode::Credential { confidence, .. }
            | AttackNode::Vulnerability { confidence, .. }
            | AttackNode::AccessLevel { confidence, .. }
            | AttackNode::Hypothesis { confidence, .. } => *confidence,
        }
    }

    /// Get the last_verified timestamp for this node.
    pub fn last_verified(&self) -> u64 {
        match self {
            AttackNode::Host { last_verified, .. }
            | AttackNode::Service { last_verified, .. }
            | AttackNode::Credential { last_verified, .. }
            | AttackNode::Vulnerability { last_verified, .. }
            | AttackNode::AccessLevel { last_verified, .. }
            | AttackNode::Hypothesis { last_verified, .. } => *last_verified,
        }
    }

    /// Check if this node is stale (not verified within the given threshold seconds).
    pub fn is_stale(&self, now: u64, threshold_secs: u64) -> bool {
        let lv = self.last_verified();
        lv == 0 || now.saturating_sub(lv) > threshold_secs
    }

    /// Returns a short label for display.
    pub fn label(&self) -> String {
        match self {
            AttackNode::Host { ip, .. } => format!("Host({ip})"),
            AttackNode::Service { name, port, .. } => format!("Service({name}:{port})"),
            AttackNode::Credential { username, .. } => format!("Cred({username})"),
            AttackNode::Vulnerability { id, .. } => format!("Vuln({id})"),
            AttackNode::AccessLevel {
                user, privilege, ..
            } => {
                format!("Access({user}@{privilege})")
            }
            AttackNode::Hypothesis { id, status, .. } => format!("Hyp({id}:{status})"),
        }
    }
}

/// A typed property graph representing the attack surface, exploited paths,
/// and hypotheses for a penetration testing engagement.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttackGraph {
    /// The underlying directed graph.
    graph: DiGraph<AttackNode, AttackEdge>,
    /// Fast lookup: "host:<ip>" | "service:<ip>:<port>" | "cred:<user>" |
    /// "vuln:<id>" | "access:<host>:<user>" | "hyp:<id>" → NodeIndex
    #[serde(
        serialize_with = "serialize_index_map",
        deserialize_with = "deserialize_index_map"
    )]
    index: HashMap<String, NodeIndex>,
}

fn serialize_index_map<S>(
    map: &HashMap<String, NodeIndex>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut m = serializer.serialize_map(Some(map.len()))?;
    for (k, v) in map {
        m.serialize_entry(k, &v.index())?;
    }
    m.end()
}

fn deserialize_index_map<'de, D>(deserializer: D) -> Result<HashMap<String, NodeIndex>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let raw: HashMap<String, usize> = HashMap::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .map(|(k, v)| (k, NodeIndex::new(v)))
        .collect())
}

impl Default for AttackGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl AttackGraph {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            index: HashMap::new(),
        }
    }

    /// Number of nodes in the graph.
    pub fn node_count(&self) -> usize {
        self.graph.node_count()
    }

    /// Number of edges in the graph.
    pub fn edge_count(&self) -> usize {
        self.graph.edge_count()
    }

    // ── Node insertion ──────────────────────────────────────────────

    /// Add a host node. Returns the index (idempotent by IP).
    pub fn add_host(&mut self, ip: &str, os: Option<&str>, confidence: f32, now: u64) -> NodeIndex {
        let key = format!("host:{ip}");
        if let Some(&idx) = self.index.get(&key) {
            // Update existing
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::Host {
                    ip: ip.to_string(),
                    os: os.map(String::from),
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let idx = self.graph.add_node(AttackNode::Host {
            ip: ip.to_string(),
            os: os.map(String::from),
            confidence,
            last_verified: now,
        });
        self.index.insert(key, idx);
        idx
    }

    /// Add a service node, automatically linked to its host via RUNS edge.
    pub fn add_service(
        &mut self,
        host_ip: &str,
        name: &str,
        port: u16,
        version: Option<&str>,
        confidence: f32,
        now: u64,
    ) -> NodeIndex {
        let key = format!("service:{host_ip}:{port}");
        if let Some(&idx) = self.index.get(&key) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::Service {
                    name: name.to_string(),
                    port,
                    version: version.map(String::from),
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let svc_idx = self.graph.add_node(AttackNode::Service {
            name: name.to_string(),
            port,
            version: version.map(String::from),
            confidence,
            last_verified: now,
        });
        self.index.insert(key, svc_idx);

        // Ensure host exists and add RUNS edge
        let host_idx = self.add_host(host_ip, None, confidence, now);
        self.graph.add_edge(host_idx, svc_idx, AttackEdge::Runs);

        svc_idx
    }

    /// Add a credential node.
    pub fn add_credential(
        &mut self,
        username: &str,
        cred_type: CredentialType,
        confidence: f32,
        now: u64,
    ) -> NodeIndex {
        let key = format!("cred:{username}");
        if let Some(&idx) = self.index.get(&key) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::Credential {
                    username: username.to_string(),
                    credential_type: cred_type,
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let idx = self.graph.add_node(AttackNode::Credential {
            username: username.to_string(),
            credential_type: cred_type,
            confidence,
            last_verified: now,
        });
        self.index.insert(key, idx);
        idx
    }

    /// Add a vulnerability node linked to a service or host.
    pub fn add_vulnerability(
        &mut self,
        id: &str,
        vuln_class: &str,
        severity: &str,
        description: &str,
        target_key: &str,
        confidence: f32,
        now: u64,
    ) -> NodeIndex {
        let key = format!("vuln:{id}");
        if let Some(&idx) = self.index.get(&key) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::Vulnerability {
                    id: id.to_string(),
                    vuln_class: vuln_class.to_string(),
                    severity: severity.to_string(),
                    description: description.to_string(),
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let vuln_idx = self.graph.add_node(AttackNode::Vulnerability {
            id: id.to_string(),
            vuln_class: vuln_class.to_string(),
            severity: severity.to_string(),
            description: description.to_string(),
            confidence,
            last_verified: now,
        });
        self.index.insert(key, vuln_idx);

        // Link target → VULNERABLE_TO → vuln
        if let Some(&target_idx) = self.index.get(target_key) {
            self.graph
                .add_edge(target_idx, vuln_idx, AttackEdge::VulnerableTo);
        }
        vuln_idx
    }

    /// Add an access level node.
    pub fn add_access_level(
        &mut self,
        host_ip: &str,
        user: &str,
        privilege: &str,
        confidence: f32,
        now: u64,
    ) -> NodeIndex {
        let key = format!("access:{host_ip}:{user}");
        if let Some(&idx) = self.index.get(&key) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::AccessLevel {
                    user: user.to_string(),
                    privilege: privilege.to_string(),
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let idx = self.graph.add_node(AttackNode::AccessLevel {
            user: user.to_string(),
            privilege: privilege.to_string(),
            confidence,
            last_verified: now,
        });
        self.index.insert(key, idx);
        idx
    }

    /// Add a hypothesis node.
    pub fn add_hypothesis(
        &mut self,
        id: &str,
        statement: &str,
        category: &str,
        status: &str,
        confidence: f32,
        now: u64,
    ) -> NodeIndex {
        let key = format!("hyp:{id}");
        if let Some(&idx) = self.index.get(&key) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                *node = AttackNode::Hypothesis {
                    id: id.to_string(),
                    statement: statement.to_string(),
                    category: category.to_string(),
                    status: status.to_string(),
                    confidence,
                    last_verified: now,
                };
            }
            return idx;
        }
        let idx = self.graph.add_node(AttackNode::Hypothesis {
            id: id.to_string(),
            statement: statement.to_string(),
            category: category.to_string(),
            status: status.to_string(),
            confidence,
            last_verified: now,
        });
        self.index.insert(key, idx);
        idx
    }

    // ── Edge insertion ──────────────────────────────────────────────

    /// Add a ROUTES_TO edge between two hosts.
    pub fn add_route(&mut self, from_ip: &str, to_ip: &str, protocol: &str) {
        let from_key = format!("host:{from_ip}");
        let to_key = format!("host:{to_ip}");
        if let (Some(&from_idx), Some(&to_idx)) =
            (self.index.get(&from_key), self.index.get(&to_key))
        {
            self.graph.add_edge(
                from_idx,
                to_idx,
                AttackEdge::RoutesTo {
                    protocol: protocol.to_string(),
                },
            );
        }
    }

    /// Add a GRANTS_ACCESS_TO edge from a source (credential or vulnerability) to an access level.
    pub fn add_grants_access(&mut self, from_key: &str, to_key: &str) {
        if let (Some(&from_idx), Some(&to_idx)) = (self.index.get(from_key), self.index.get(to_key))
        {
            self.graph
                .add_edge(from_idx, to_idx, AttackEdge::GrantsAccessTo);
        }
    }

    /// Add a CONFIRMS edge from a source node to a hypothesis.
    pub fn add_confirms(&mut self, from_key: &str, hypothesis_id: &str) {
        let hyp_key = format!("hyp:{hypothesis_id}");
        if let (Some(&from_idx), Some(&hyp_idx)) =
            (self.index.get(from_key), self.index.get(&hyp_key))
        {
            self.graph.add_edge(from_idx, hyp_idx, AttackEdge::Confirms);
        }
    }

    /// Add a DISPROVES edge from a source node to a hypothesis.
    pub fn add_disproves(&mut self, from_key: &str, hypothesis_id: &str) {
        let hyp_key = format!("hyp:{hypothesis_id}");
        if let (Some(&from_idx), Some(&hyp_idx)) =
            (self.index.get(from_key), self.index.get(&hyp_key))
        {
            self.graph
                .add_edge(from_idx, hyp_idx, AttackEdge::Disproves);
        }
    }

    // ── Queries ─────────────────────────────────────────────────────

    /// Find the shortest path (by hop count) from an external host to a target node.
    /// Returns the ordered list of node labels along the path, or None if unreachable.
    pub fn shortest_path(&self, from_key: &str, to_key: &str) -> Option<Vec<String>> {
        let from_idx = *self.index.get(from_key)?;
        let to_idx = *self.index.get(to_key)?;

        // BFS
        use std::collections::VecDeque;
        let mut visited = vec![false; self.graph.node_count()];
        let mut parent: Vec<Option<NodeIndex>> = vec![None; self.graph.node_count()];
        let mut queue = VecDeque::new();

        visited[from_idx.index()] = true;
        queue.push_back(from_idx);

        while let Some(current) = queue.pop_front() {
            if current == to_idx {
                // Reconstruct path
                let mut path = Vec::new();
                let mut cursor = Some(to_idx);
                while let Some(c) = cursor {
                    if let Some(node) = self.graph.node_weight(c) {
                        path.push(node.label());
                    }
                    cursor = parent[c.index()];
                }
                path.reverse();
                return Some(path);
            }
            for edge in self.graph.edges(current) {
                let neighbor = edge.target();
                if !visited[neighbor.index()] {
                    visited[neighbor.index()] = true;
                    parent[neighbor.index()] = Some(current);
                    queue.push_back(neighbor);
                }
            }
        }
        None
    }

    /// Return all hypothesis nodes that have status "Proposed" (untested).
    pub fn untested_hypotheses(&self) -> Vec<&AttackNode> {
        self.graph
            .node_weights()
            .filter(|n| matches!(n, AttackNode::Hypothesis { status, .. } if status == "Proposed"))
            .collect()
    }

    /// Return all hypothesis nodes matching any of the given statuses.
    pub fn hypotheses_by_status(&self, statuses: &[&str]) -> Vec<&AttackNode> {
        self.graph
            .node_weights()
            .filter(|n| {
                matches!(n, AttackNode::Hypothesis { status, .. } if statuses.contains(&status.as_str()))
            })
            .collect()
    }

    /// Return all nodes of a specific type.
    pub fn nodes_of_type(&self, type_name: &str) -> Vec<&AttackNode> {
        self.graph
            .node_weights()
            .filter(|n| match (type_name, n) {
                ("Host", AttackNode::Host { .. }) => true,
                ("Service", AttackNode::Service { .. }) => true,
                ("Credential", AttackNode::Credential { .. }) => true,
                ("Vulnerability", AttackNode::Vulnerability { .. }) => true,
                ("AccessLevel", AttackNode::AccessLevel { .. }) => true,
                ("Hypothesis", AttackNode::Hypothesis { .. }) => true,
                _ => false,
            })
            .collect()
    }

    /// Return all stale nodes (not verified within threshold_secs of now).
    pub fn stale_nodes(&self, now: u64, threshold_secs: u64) -> Vec<&AttackNode> {
        self.graph
            .node_weights()
            .filter(|n| n.is_stale(now, threshold_secs))
            .collect()
    }

    /// Get a node by its lookup key.
    pub fn get_node(&self, key: &str) -> Option<&AttackNode> {
        let idx = self.index.get(key)?;
        self.graph.node_weight(*idx)
    }

    /// Get all neighbors (outgoing edges) of a node.
    pub fn neighbors(&self, key: &str) -> Vec<(&AttackEdge, &AttackNode)> {
        let Some(&idx) = self.index.get(key) else {
            return Vec::new();
        };
        self.graph
            .edges(idx)
            .filter_map(|e| {
                let target = self.graph.node_weight(e.target())?;
                Some((e.weight(), target))
            })
            .collect()
    }

    /// Check if a key exists in the graph.
    pub fn contains(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> u64 {
        1000
    }

    #[test]
    fn test_new_graph_is_empty() {
        let g = AttackGraph::new();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn test_add_host_and_service() {
        let mut g = AttackGraph::new();
        let h = g.add_host("10.0.0.1", Some("Linux"), 0.9, now());
        assert_eq!(h.index(), 0);

        let s = g.add_service("10.0.0.1", "http", 80, Some("Apache/2.4"), 0.95, now());
        assert_eq!(g.node_count(), 2);
        assert_eq!(g.edge_count(), 1); // RUNS edge
        assert_ne!(h, s);
    }

    #[test]
    fn test_idempotent_host_insertion() {
        let mut g = AttackGraph::new();
        let h1 = g.add_host("10.0.0.1", None, 0.5, now());
        let h2 = g.add_host("10.0.0.1", Some("Linux"), 0.9, now());
        assert_eq!(h1, h2);
        assert_eq!(g.node_count(), 1);
        // Verify updated
        let node = g.get_node("host:10.0.0.1").unwrap();
        assert_eq!(node.confidence(), 0.9);
    }

    #[test]
    fn test_service_creates_host_if_missing() {
        let mut g = AttackGraph::new();
        g.add_service("10.0.0.5", "ssh", 22, None, 0.8, now());
        assert_eq!(g.node_count(), 2); // host + service
        assert!(g.contains("host:10.0.0.5"));
        assert!(g.contains("service:10.0.0.5:22"));
    }

    #[test]
    fn test_vulnerability_linked_to_service() {
        let mut g = AttackGraph::new();
        g.add_service("10.0.0.1", "http", 80, None, 0.9, now());
        g.add_vulnerability(
            "sqli-login",
            "SQL Injection",
            "High",
            "Login form SQL injection",
            "service:10.0.0.1:80",
            0.85,
            now(),
        );
        assert_eq!(g.node_count(), 3); // host, service, vuln
        assert_eq!(g.edge_count(), 2); // RUNS + VULNERABLE_TO
    }

    #[test]
    fn test_credential_and_grants_access() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 0.9, now());
        g.add_credential("admin", CredentialType::Password, 1.0, now());
        g.add_access_level("10.0.0.1", "admin", "root", 1.0, now());
        g.add_grants_access("cred:admin", "access:10.0.0.1:admin");
        assert_eq!(g.node_count(), 3);
        assert_eq!(g.edge_count(), 1); // GRANTS_ACCESS_TO
    }

    #[test]
    fn test_route_between_hosts() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 0.9, now());
        g.add_host("10.0.1.1", None, 0.8, now());
        g.add_route("10.0.0.1", "10.0.1.1", "tcp");
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn test_hypothesis_confirms_disproves() {
        let mut g = AttackGraph::new();
        g.add_service("10.0.0.1", "http", 80, None, 0.9, now());
        g.add_hypothesis("h1", "XSS in search", "Input", "Proposed", 0.5, now());
        g.add_vulnerability(
            "xss-1",
            "XSS",
            "Medium",
            "Reflected XSS",
            "service:10.0.0.1:80",
            0.9,
            now(),
        );
        g.add_confirms("vuln:xss-1", "h1");
        assert_eq!(g.edge_count(), 3); // RUNS + VULNERABLE_TO + CONFIRMS
    }

    #[test]
    fn test_shortest_path() {
        let mut g = AttackGraph::new();
        g.add_host("external", None, 1.0, now());
        g.add_host("dmz", None, 0.9, now());
        g.add_host("internal", None, 0.8, now());
        g.add_route("external", "dmz", "tcp");
        g.add_route("dmz", "internal", "tcp");

        let path = g.shortest_path("host:external", "host:internal");
        assert!(path.is_some());
        let labels = path.unwrap();
        assert_eq!(labels.len(), 3);
        assert_eq!(labels[0], "Host(external)");
        assert_eq!(labels[2], "Host(internal)");
    }

    #[test]
    fn test_shortest_path_no_route() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 1.0, now());
        g.add_host("10.0.0.2", None, 0.9, now());
        // No route
        let path = g.shortest_path("host:10.0.0.1", "host:10.0.0.2");
        assert!(path.is_none());
    }

    #[test]
    fn test_untested_hypotheses() {
        let mut g = AttackGraph::new();
        g.add_hypothesis("h1", "SQLi in login", "Input", "Proposed", 0.5, now());
        g.add_hypothesis("h2", "XSS in search", "Input", "Confirmed", 0.9, now());
        g.add_hypothesis("h3", "IDOR on /api", "Logic", "Proposed", 0.4, now());

        let untested = g.untested_hypotheses();
        assert_eq!(untested.len(), 2);
    }

    #[test]
    fn test_hypotheses_by_status() {
        let mut g = AttackGraph::new();
        g.add_hypothesis("h1", "SQLi", "Input", "Proposed", 0.5, now());
        g.add_hypothesis("h2", "XSS", "Input", "Confirmed", 0.9, now());
        g.add_hypothesis("h3", "IDOR", "Logic", "Exploited", 0.95, now());

        let confirmed = g.hypotheses_by_status(&["Confirmed", "Exploited"]);
        assert_eq!(confirmed.len(), 2);
    }

    #[test]
    fn test_stale_nodes() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 0.9, 100); // old
        g.add_host("10.0.0.2", None, 0.9, 900); // recent

        let stale = g.stale_nodes(1000, 500);
        assert_eq!(stale.len(), 1);
        assert_eq!(stale[0].label(), "Host(10.0.0.1)");
    }

    #[test]
    fn test_nodes_of_type() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 0.9, now());
        g.add_service("10.0.0.1", "http", 80, None, 0.9, now());
        g.add_credential("admin", CredentialType::Password, 1.0, now());

        assert_eq!(g.nodes_of_type("Host").len(), 1);
        assert_eq!(g.nodes_of_type("Service").len(), 1);
        assert_eq!(g.nodes_of_type("Credential").len(), 1);
        assert_eq!(g.nodes_of_type("Vulnerability").len(), 0);
    }

    #[test]
    fn test_neighbors() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", None, 0.9, now());
        g.add_service("10.0.0.1", "http", 80, None, 0.9, now());
        g.add_service("10.0.0.1", "ssh", 22, None, 0.9, now());

        let neighbors = g.neighbors("host:10.0.0.1");
        assert_eq!(neighbors.len(), 2);
        assert!(neighbors.iter().all(|(e, _)| *e == &AttackEdge::Runs));
    }

    #[test]
    fn test_serialization_roundtrip() {
        let mut g = AttackGraph::new();
        g.add_host("10.0.0.1", Some("Linux"), 0.9, now());
        g.add_service("10.0.0.1", "http", 80, Some("nginx"), 0.95, now());
        g.add_hypothesis("h1", "SQLi test", "Input", "Proposed", 0.5, now());

        let json = serde_json::to_string(&g).unwrap();
        let g2: AttackGraph = serde_json::from_str(&json).unwrap();

        assert_eq!(g2.node_count(), g.node_count());
        assert_eq!(g2.edge_count(), g.edge_count());
        assert!(g2.contains("host:10.0.0.1"));
        assert!(g2.contains("service:10.0.0.1:80"));
        assert!(g2.contains("hyp:h1"));
    }

    #[test]
    fn test_complex_multi_network_scenario() {
        let mut g = AttackGraph::new();

        // Network 1: DMZ
        g.add_host("10.0.1.1", Some("Linux"), 0.9, now());
        g.add_service("10.0.1.1", "http", 80, Some("Apache"), 0.95, now());
        g.add_service("10.0.1.1", "ssh", 22, None, 0.9, now());

        // Network 2: Internal
        g.add_host("10.0.2.1", Some("Windows"), 0.8, now());
        g.add_service("10.0.2.1", "smb", 445, None, 0.85, now());
        g.add_service("10.0.2.1", "rdp", 3389, None, 0.85, now());

        // Network 3: Database
        g.add_host("10.0.3.1", Some("Linux"), 0.7, now());
        g.add_service("10.0.3.1", "mysql", 3306, Some("MySQL 8.0"), 0.8, now());

        // Routes
        g.add_route("10.0.1.1", "10.0.2.1", "tcp");
        g.add_route("10.0.2.1", "10.0.3.1", "tcp");

        // Attack path: vuln on DMZ web → credential → access internal → access DB
        g.add_vulnerability(
            "sqli-1",
            "SQL Injection",
            "Critical",
            "SQLi in login form",
            "service:10.0.1.1:80",
            0.9,
            now(),
        );
        g.add_credential("dbadmin", CredentialType::Password, 0.95, now());
        g.add_access_level("10.0.2.1", "admin", "Administrator", 0.9, now());
        g.add_grants_access("cred:dbadmin", "access:10.0.2.1:admin");

        // Verify path from DMZ to DB network
        let path = g.shortest_path("host:10.0.1.1", "host:10.0.3.1");
        assert!(path.is_some());
        let labels = path.unwrap();
        assert_eq!(labels.len(), 3);

        // Verify graph counts
        assert_eq!(g.nodes_of_type("Host").len(), 3);
        assert_eq!(g.nodes_of_type("Service").len(), 5);
        assert_eq!(g.nodes_of_type("Vulnerability").len(), 1);
    }
}
