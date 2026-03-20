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

    // 4. Add hosts and ports
    rt(
        &[
            "kb",
            "add-host",
            "10.10.10.1",
            "--os",
            "Linux",
            "--hostname",
            "target",
        ],
        dir,
    );
    rt(
        &[
            "kb",
            "add-port",
            "10.10.10.1",
            "22",
            "--service",
            "ssh",
            "--version",
            "OpenSSH 8.9",
        ],
        dir,
    );
    rt(
        &[
            "kb",
            "add-port",
            "10.10.10.1",
            "80",
            "--service",
            "http",
            "--version",
            "nginx 1.18",
        ],
        dir,
    );

    // 5. Verify KB state
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

    // 7. Create hypotheses
    rt(
        &[
            "hypothesis",
            "create",
            "SQLi in login form",
            "--category",
            "I",
            "--priority",
            "high",
            "--confidence",
            "0.8",
        ],
        dir,
    );
    rt(
        &[
            "hypothesis",
            "create",
            "Debug console exposed on :80",
            "--category",
            "C",
            "--priority",
            "critical",
            "--confidence",
            "0.6",
        ],
        dir,
    );

    let hyps = rt_json(&["hypothesis", "list", "--json"], dir);
    assert_eq!(hyps.as_array().unwrap().len(), 2);

    // 8. Update hypothesis — confirm one
    let hyp_id = hyps[0]["id"].as_i64().unwrap();
    rt(
        &[
            "hypothesis",
            "update",
            &hyp_id.to_string(),
            "--status",
            "confirmed",
        ],
        dir,
    );

    let confirmed = rt_json(
        &["hypothesis", "list", "--status", "confirmed", "--json"],
        dir,
    );
    assert_eq!(confirmed.as_array().unwrap().len(), 1);

    // 9. Add evidence
    rt(
        &[
            "evidence",
            "add",
            "--finding",
            "SQL error in response with single quote",
            "--hypothesis",
            &hyp_id.to_string(),
            "--severity",
            "critical",
            "--poc",
            "curl http://10.10.10.1/login -d 'user=admin\\''",
        ],
        dir,
    );

    let evidence = rt_json(&["evidence", "list", "--json"], dir);
    assert_eq!(evidence.as_array().unwrap().len(), 1);
    assert_eq!(evidence[0]["severity"], "critical");

    // 10. Add credentials and flag
    rt(
        &[
            "kb",
            "add-cred",
            "admin",
            "--pass",
            "admin123",
            "--service",
            "mysql",
            "--host",
            "10.10.10.1",
        ],
        dir,
    );
    rt(
        &[
            "kb",
            "add-flag",
            "HTB{e2e_test_flag}",
            "--source",
            "/home/user/user.txt",
        ],
        dir,
    );

    let flags = rt_json(&["kb", "flags", "--json"], dir);
    assert_eq!(flags[0]["value"], "HTB{e2e_test_flag}");

    let creds = rt_json(&["kb", "creds", "--json"], dir);
    assert_eq!(creds[0]["username"], "admin");

    // 11. Add access
    rt(
        &[
            "kb",
            "add-access",
            "10.10.10.1",
            "admin",
            "user",
            "--method",
            "ssh",
        ],
        dir,
    );

    // 12. Search
    let search = rt_json(&["kb", "search", "admin", "--json"], dir);
    assert!(!search.as_array().unwrap().is_empty());

    // 13. Scope check
    let scope_in = rt(&["scope", "check", "10.10.10.5"], dir);
    assert!(scope_in.status.success());
    let scope_out = rt(&["scope", "check", "192.168.1.1"], dir);
    assert!(!scope_out.status.success());

    // 14. Command history
    let history = rt_json(&["kb", "history", "--json"], dir);
    assert!(!history.as_array().unwrap().is_empty());

    // 15. Generate report
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

    // 16. Session info
    let session = rt_json(&["session", "active", "--json"], dir);
    assert_eq!(session["target"], "10.10.10.1");

    // 17. Env outputs valid shell code
    let env_out = rt(&["env"], dir);
    assert!(env_out.status.success());
    let env_str = String::from_utf8_lossy(&env_out.stdout);
    assert!(env_str.contains("alias nmap"));
    assert!(env_str.contains("RT_WORKSPACE"));
    assert!(env_str.contains("rt_deactivate"));

    // 18. Config
    let config_out = rt(&["config", "list"], dir);
    assert!(config_out.status.success());

    // 19. Evidence export
    let export = rt(&["evidence", "export", "--json"], dir);
    assert!(export.status.success());

    // 20. Notes
    rt(
        &["kb", "add-note", "SSH credentials found via SQLi dump"],
        dir,
    );
    let notes = rt_json(&["kb", "notes", "--json"], dir);
    assert_eq!(notes.as_array().unwrap().len(), 1);

    // 21. Final status — verify everything accumulated
    let final_status = rt_json(&["status", "--json"], dir);
    assert_eq!(final_status["hosts"], 1);
    assert_eq!(final_status["ports"], 2);
    assert_eq!(final_status["flags"], 1);
    assert!(final_status["creds"].as_i64().unwrap() >= 1);
}
