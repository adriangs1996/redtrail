use std::process::Command;

fn rt(args: &[&str], dir: &std::path::Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_rt"))
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap()
}

fn rt_json(args: &[&str], dir: &std::path::Path) -> serde_json::Value {
    let out = rt(args, dir);
    assert!(
        out.status.success(),
        "cmd {:?} failed: {}",
        args,
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn test_full_pentest_workflow() {
    let tmp = tempfile::tempdir().unwrap();
    let dir = tmp.path();

    // 1. Init workspace
    let out = rt(
        &[
            "init",
            "--target",
            "10.10.10.1",
            "--goal",
            "capture-flags",
            "--scope",
            "10.10.10.0/24",
        ],
        dir,
    );
    assert!(out.status.success());

    // 2. Verify workspace structure
    assert!(dir.join(".redtrail/redtrail.db").exists());
    assert!(dir.join(".redtrail/config.toml").exists());
    assert!(dir.join(".redtrail/aliases.sh").exists());

    // 3. Proxy mode — execute a command
    let out = rt(
        &[
            "echo",
            "PORT STATE SERVICE\n22/tcp open ssh\n80/tcp open http",
        ],
        dir,
    );
    assert!(out.status.success());

    // 4. Insert data via SQL (writes now go through dispatcher/agent, not CLI)
    rt(
        &[
            "sql",
            "INSERT OR IGNORE INTO hosts (session_id, ip, os, hostname) \
             SELECT id, '10.10.10.1', 'Linux', 'target' FROM sessions LIMIT 1",
        ],
        dir,
    );
    rt(
        &[
            "sql",
            "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) \
             SELECT s.id, h.id, 22, 'tcp', 'ssh', 'OpenSSH 8.9' \
             FROM sessions s JOIN hosts h ON h.session_id = s.id LIMIT 1",
        ],
        dir,
    );
    rt(
        &[
            "sql",
            "INSERT OR IGNORE INTO ports (session_id, host_id, port, protocol, service, version) \
             SELECT s.id, h.id, 80, 'tcp', 'http', 'nginx 1.18' \
             FROM sessions s JOIN hosts h ON h.session_id = s.id LIMIT 1",
        ],
        dir,
    );

    // 5. Verify KB state (read-only CLI)
    let hosts = rt_json(&["kb", "hosts", "--json"], dir);
    assert_eq!(hosts.as_array().unwrap().len(), 1);
    assert_eq!(hosts[0]["ip"], "10.10.10.1");
    assert_eq!(hosts[0]["os"], "Linux");

    let ports = rt_json(&["kb", "ports", "--json"], dir);
    assert_eq!(ports.as_array().unwrap().len(), 2);

    // 6. Check status
    let status = rt_json(&["status", "--json"], dir);
    assert_eq!(status["target"], "10.10.10.1");
    assert_eq!(status["hosts"], 1);
    assert_eq!(status["ports"], 2);

    // 7. Insert hypotheses via SQL
    rt(
        &[
            "sql",
            "INSERT INTO hypotheses (session_id, statement, category, priority, confidence) \
             SELECT id, 'SQLi in login form', 'I', 'high', 0.8 FROM sessions LIMIT 1",
        ],
        dir,
    );
    rt(
        &[
            "sql",
            "INSERT INTO hypotheses (session_id, statement, category, priority, confidence) \
             SELECT id, 'Debug console exposed on :80', 'C', 'critical', 0.6 FROM sessions LIMIT 1",
        ],
        dir,
    );

    let hyps = rt_json(&["hypothesis", "list", "--json"], dir);
    assert_eq!(hyps.as_array().unwrap().len(), 2);

    // 8. Update hypothesis via SQL, verify through read CLI
    let hyp_id = hyps[0]["id"].as_i64().unwrap();
    rt(
        &[
            "sql",
            &format!(
                "UPDATE hypotheses SET status = 'confirmed', resolved_at = datetime('now') WHERE id = {hyp_id}"
            ),
        ],
        dir,
    );

    let confirmed = rt_json(
        &["hypothesis", "list", "--status", "confirmed", "--json"],
        dir,
    );
    assert_eq!(confirmed.as_array().unwrap().len(), 1);

    // 9. Insert evidence via SQL
    rt(
        &[
            "sql",
            &format!(
                "INSERT INTO evidence (session_id, hypothesis_id, finding, severity, poc) \
                 SELECT id, {hyp_id}, 'SQL error in response with single quote', 'critical', \
                 'curl http://10.10.10.1/login' FROM sessions LIMIT 1"
            ),
        ],
        dir,
    );

    let evidence = rt_json(&["evidence", "list", "--json"], dir);
    assert_eq!(evidence.as_array().unwrap().len(), 1);
    assert_eq!(evidence[0]["severity"], "critical");

    // 10. Insert credentials and flag via SQL
    rt(
        &[
            "sql",
            "INSERT INTO credentials (session_id, username, password, service, host) \
             SELECT id, 'admin', 'admin123', 'mysql', '10.10.10.1' FROM sessions LIMIT 1",
        ],
        dir,
    );
    rt(
        &[
            "sql",
            "INSERT INTO flags (session_id, value, source) \
             SELECT id, 'HTB{e2e_test_flag}', '/home/user/user.txt' FROM sessions LIMIT 1",
        ],
        dir,
    );

    let flags = rt_json(&["kb", "flags", "--json"], dir);
    assert_eq!(flags[0]["value"], "HTB{e2e_test_flag}");

    let creds = rt_json(&["kb", "creds", "--json"], dir);
    assert_eq!(creds[0]["username"], "admin");

    // 11. Scope check
    let scope_in = rt(&["scope", "check", "10.10.10.5"], dir);
    assert!(scope_in.status.success());
    let scope_out = rt(&["scope", "check", "192.168.1.1"], dir);
    assert!(!scope_out.status.success());

    // 12. Command history
    let history = rt_json(&["kb", "history", "--json"], dir);
    assert!(!history.as_array().unwrap().is_empty());

    // 13. Generate report
    let report_path = dir.join("report.md");
    rt(
        &[
            "report",
            "generate",
            "--output",
            report_path.to_str().unwrap(),
        ],
        dir,
    );
    assert!(report_path.exists());
    let report = std::fs::read_to_string(&report_path).unwrap();
    assert!(report.contains("10.10.10.1"));
    assert!(report.contains("HTB{e2e_test_flag}"));
    assert!(report.contains("SQLi"));

    // 14. Session info
    let session = rt_json(&["session", "active", "--json"], dir);
    assert_eq!(session["target"], "10.10.10.1");

    // 15. Env outputs valid shell code
    let env_out = rt(&["env"], dir);
    assert!(env_out.status.success());
    let env_str = String::from_utf8_lossy(&env_out.stdout);
    assert!(env_str.contains("alias nmap"));
    assert!(env_str.contains("RT_WORKSPACE"));
    assert!(env_str.contains("rt_deactivate"));

    // 16. Config
    let config_out = rt(&["config", "list"], dir);
    assert!(config_out.status.success());

    // 17. Evidence export
    let export = rt(&["evidence", "export", "--json"], dir);
    assert!(export.status.success());

    // 18. Final status — verify everything accumulated
    let final_status = rt_json(&["status", "--json"], dir);
    assert_eq!(final_status["hosts"], 1);
    assert_eq!(final_status["ports"], 2);
    assert_eq!(final_status["flags"], 1);
    assert!(final_status["creds"].as_i64().unwrap() >= 1);
}
