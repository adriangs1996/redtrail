use redtrail::core::extractor;

fn load_fixture(name: &str) -> String {
    std::fs::read_to_string(format!("eval/tests/fixtures/{name}")).unwrap()
}

// --- detect_tool tests ---

#[test]
fn detect_tool_simple() {
    assert_eq!(extractor::detect_tool("nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_with_hint() {
    assert_eq!(extractor::detect_tool("foo bar", Some("nmap")).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_sudo() {
    assert_eq!(extractor::detect_tool("sudo nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_proxychains() {
    assert_eq!(extractor::detect_tool("proxychains nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_env_vars() {
    assert_eq!(extractor::detect_tool("MY_VAR=foo sudo nmap -sV 10.10.10.1", None).as_deref(), Some("nmap"));
}

#[test]
fn detect_tool_skips_multiple_env_vars() {
    assert_eq!(extractor::detect_tool("A=1 B=2 gobuster dir -u http://target", None).as_deref(), Some("gobuster"));
}

#[test]
fn detect_tool_empty_command() {
    assert_eq!(extractor::detect_tool("", None), None);
}

// --- nmap extractor tests ---

#[test]
fn synthetize_nmap_fixture() {
    let output = load_fixture("nmap-scan.txt");
    let result = extractor::synthetize("nmap -sV -sC -p- 10.10.10.42", Some("nmap"), &output);

    let hosts: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "host").collect();
    assert_eq!(hosts.len(), 1);
    assert_eq!(hosts[0].key, "host:10.10.10.42");
    assert_eq!(hosts[0].attributes["ip"], "10.10.10.42");

    let services: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "service").collect();
    assert!(services.len() >= 4, "expected at least 4 services, got {}", services.len());

    // SSH
    let ssh = services.iter().find(|s| s.attributes["port"] == 22).unwrap();
    assert_eq!(ssh.key, "service:10.10.10.42:22/tcp");
    assert_eq!(ssh.attributes["service"], "ssh");

    // HTTP
    let http = services.iter().find(|s| s.attributes["port"] == 80).unwrap();
    assert_eq!(http.attributes["service"], "http");

    // MySQL
    let mysql = services.iter().find(|s| s.attributes["port"] == 3306).unwrap();
    assert_eq!(mysql.attributes["service"], "mysql");

    // Relations
    let runs_on: Vec<_> = result.relations.iter().filter(|r| r.relation_type == "runs_on").collect();
    assert!(runs_on.len() >= 4);
    assert!(runs_on.iter().all(|r| r.to_key == "host:10.10.10.42"));
}

#[test]
fn synthetize_nmap_with_hostname() {
    let output = "Nmap scan report for board.htb (10.10.10.42)\n22/tcp open ssh OpenSSH 8.9\n";
    let result = extractor::synthetize("nmap 10.10.10.42", Some("nmap"), output);

    let host = result.facts.iter().find(|f| f.fact_type == "host").unwrap();
    assert_eq!(host.attributes["hostname"], "board.htb");
    assert_eq!(host.attributes["ip"], "10.10.10.42");
}

#[test]
fn synthetize_nmap_os_detection() {
    let output = "Nmap scan report for 10.10.10.42\n22/tcp open ssh\nOS details: Linux 5.4\n";
    let result = extractor::synthetize("nmap -O 10.10.10.42", Some("nmap"), output);

    let os = result.facts.iter().find(|f| f.fact_type == "os_info").unwrap();
    assert_eq!(os.attributes["os"], "Linux 5.4");
}

#[test]
fn synthetize_nmap_empty_output() {
    let result = extractor::synthetize("nmap 10.10.10.1", Some("nmap"), "");
    assert!(result.is_empty());
}

#[test]
fn synthetize_unknown_tool() {
    let result = extractor::synthetize("curl http://example.com", Some("curl"), "hello world");
    assert!(result.is_empty());
}

// --- web_enum extractor tests ---

#[test]
fn synthetize_gobuster_fixture() {
    let output = load_fixture("gobuster-scan.txt");
    let result = extractor::synthetize(
        "gobuster dir -u http://10.10.10.42 -w /usr/share/wordlists/dirb/common.txt",
        Some("gobuster"),
        &output,
    );

    let paths: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "web_path").collect();
    assert!(paths.len() >= 9, "expected at least 9 paths, got {}", paths.len());

    let admin = paths.iter().find(|p| p.attributes["path"] == "/admin").unwrap();
    assert_eq!(admin.attributes["status_code"], 301);
    assert_eq!(admin.attributes["redirect_to"], "http://10.10.10.42/admin/");

    let api = paths.iter().find(|p| p.attributes["path"] == "/api").unwrap();
    assert_eq!(api.attributes["status_code"], 200);
    assert_eq!(api.attributes["content_length"], 1245);

    assert!(!result.relations.is_empty());
}

#[test]
fn synthetize_gobuster_extracts_target_from_command() {
    let output = "/test                 (Status: 200) [Size: 100]\n";
    let result = extractor::synthetize(
        "gobuster dir -u http://10.10.10.42:8080 -w wordlist.txt",
        Some("gobuster"),
        output,
    );

    let path = &result.facts[0];
    assert_eq!(path.attributes["ip"], "10.10.10.42");
    assert_eq!(path.attributes["port"], 8080);
}

#[test]
fn synthetize_ffuf_format() {
    let output = "admin                   [Status: 200, Size: 1234, Words: 56, Lines: 12]\n";
    let result = extractor::synthetize(
        "ffuf -u http://10.10.10.1/FUZZ -w wordlist.txt",
        Some("ffuf"),
        output,
    );

    assert!(!result.facts.is_empty());
    let path = &result.facts[0];
    assert_eq!(path.fact_type, "web_path");
    assert_eq!(path.attributes["status_code"], 200);
}

#[test]
fn synthetize_gobuster_empty_output() {
    let result = extractor::synthetize("gobuster dir -u http://10.10.10.1 -w w.txt", Some("gobuster"), "");
    assert!(result.is_empty());
}

// --- hydra extractor tests ---

#[test]
fn synthetize_hydra_basic() {
    let output = "[22][ssh] host: 10.10.10.1   login: admin   password: secret123\n\
                  [22][ssh] host: 10.10.10.1   login: root   password: toor\n";
    let result = extractor::synthetize("hydra -l admin -P passwords.txt ssh://10.10.10.1", Some("hydra"), output);

    let creds: Vec<_> = result.facts.iter().filter(|f| f.fact_type == "credential").collect();
    assert_eq!(creds.len(), 2);

    let admin = creds.iter().find(|c| c.attributes["username"] == "admin").unwrap();
    assert_eq!(admin.attributes["password"], "secret123");
    assert_eq!(admin.attributes["service"], "ssh");
    assert_eq!(admin.attributes["ip"], "10.10.10.1");

    let auth_rels: Vec<_> = result.relations.iter()
        .filter(|r| r.relation_type == "authenticates_to")
        .collect();
    assert_eq!(auth_rels.len(), 2);
}

#[test]
fn synthetize_hydra_no_results() {
    let output = "Hydra (https://github.com/vanhauser-thc/thc-hydra)\n\
                  [DATA] attacking ssh://10.10.10.1:22/\n\
                  0 valid password found\n";
    let result = extractor::synthetize("hydra -l admin -P pass.txt ssh://10.10.10.1", Some("hydra"), output);
    assert!(result.facts.is_empty());
}
