use redtrail::core::db::CommandRow;
use redtrail::extract::generic::GenericExtractor;
use redtrail::extract::types::DomainExtractor;

fn cmd_with_stdout(stdout: &str) -> CommandRow {
    CommandRow {
        id: "test-id".into(),
        session_id: "sess".into(),
        command_raw: "some-command".into(),
        command_binary: Some("some-command".into()),
        stdout: Some(stdout.into()),
        cwd: Some("/home/user/project".into()),
        source: "human".into(),
        timestamp_start: 1000,
        ..Default::default()
    }
}

#[test]
fn can_handle_always_returns_true() {
    let ext = GenericExtractor;
    assert!(ext.can_handle("git", Some("status")));
    assert!(ext.can_handle("anything", None));
    assert!(ext.can_handle("", None));
}

#[test]
fn extracts_absolute_file_paths() {
    let cmd = cmd_with_stdout("error in /home/user/project/src/main.rs:42\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "file" && e.canonical_key.contains("main.rs")),
        "expected file entity containing main.rs, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn strips_line_number_from_canonical_key() {
    let cmd = cmd_with_stdout("error in /home/user/project/src/main.rs:42:10\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    let file_entity = result.entities.iter().find(|e| e.entity_type == "file" && e.canonical_key.contains("main.rs"));
    assert!(file_entity.is_some(), "expected file entity");
    let key = &file_entity.unwrap().canonical_key;
    assert!(!key.contains(":42"), "canonical_key should not contain line number, got: {key}");
    assert!(!key.contains(":10"), "canonical_key should not contain column number, got: {key}");
}

#[test]
fn extracts_relative_file_paths() {
    let cmd = cmd_with_stdout("warning: ./src/lib.rs unused import\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "file"),
        "expected file entity, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn resolves_relative_path_against_cwd() {
    let cmd = cmd_with_stdout("warning: ./src/lib.rs unused import\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    let file = result.entities.iter().find(|e| e.entity_type == "file");
    assert!(file.is_some());
    // Resolved path should be absolute (starts with /)
    assert!(
        file.unwrap().canonical_key.starts_with('/'),
        "resolved relative path should be absolute, got: {}",
        file.unwrap().canonical_key
    );
}

#[test]
fn extracts_parent_relative_file_paths() {
    let cmd = cmd_with_stdout("see ../config/settings.yaml for details\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "file"),
        "expected file entity for ../path, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn extracts_ip_addresses() {
    let cmd = cmd_with_stdout("Connected to 192.168.1.100 on port 8080\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "ip_address" && e.canonical_key == "192.168.1.100"),
        "expected ip_address entity 192.168.1.100, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn no_false_positive_on_version_numbers() {
    let cmd = cmd_with_stdout("node v18.2.0 installed\nversion 1.2.3\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.entity_type == "ip_address"),
        "version numbers should not be detected as IPs, got: {:?}",
        result.entities.iter().filter(|e| e.entity_type == "ip_address").map(|e| &e.canonical_key).collect::<Vec<_>>()
    );
}

#[test]
fn excludes_broadcast_ip() {
    let cmd = cmd_with_stdout("listen on 255.255.255.255\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.entity_type == "ip_address" && e.canonical_key == "255.255.255.255"),
        "255.255.255.255 should be excluded"
    );
}

#[test]
fn excludes_zero_ip() {
    let cmd = cmd_with_stdout("binding to 0.0.0.0\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.entity_type == "ip_address" && e.canonical_key == "0.0.0.0"),
        "0.0.0.0 should be excluded"
    );
}

#[test]
fn validates_ip_octets() {
    // 999.999.999.999 is not a valid IP
    let cmd = cmd_with_stdout("address 999.999.999.999\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.entity_type == "ip_address" && e.canonical_key == "999.999.999.999"),
        "invalid octets should be rejected"
    );
}

#[test]
fn extracts_urls() {
    let cmd = cmd_with_stdout("Downloading https://example.com/file.tar.gz\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "url"),
        "expected url entity, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn trims_trailing_punctuation_from_url() {
    let cmd = cmd_with_stdout("see https://example.com/page, for details\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    let url_entity = result.entities.iter().find(|e| e.entity_type == "url");
    assert!(url_entity.is_some(), "expected url entity");
    let key = &url_entity.unwrap().canonical_key;
    assert!(!key.ends_with(','), "trailing comma should be trimmed from URL, got: {key}");
}

#[test]
fn trims_trailing_period_from_url() {
    let cmd = cmd_with_stdout("visit https://example.com/path.\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    let url_entity = result.entities.iter().find(|e| e.entity_type == "url");
    assert!(url_entity.is_some(), "expected url entity");
    let key = &url_entity.unwrap().canonical_key;
    assert!(!key.ends_with('.'), "trailing period should be trimmed from URL, got: {key}");
}

#[test]
fn extracts_ports_from_urls() {
    let cmd = cmd_with_stdout("Listening on http://127.0.0.1:3000\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "port" && e.canonical_key.contains("3000")),
        "expected port entity for 3000, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn extracts_ports_from_context() {
    let cmd = cmd_with_stdout("Server started on port 8080\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "port" && e.canonical_key.contains("8080")),
        "expected port entity for 8080, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn extracts_port_from_listening_colon_syntax() {
    let cmd = cmd_with_stdout("Listening on :8080\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "port" && e.canonical_key.contains("8080")),
        "expected port entity for :8080, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn port_validates_range() {
    // Port 99999 is out of range
    let cmd = cmd_with_stdout("Listening on port 99999\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.entity_type == "port" && e.canonical_key == "port:99999"),
        "port 99999 is out of valid range and should be excluded"
    );
}

#[test]
fn empty_stdout_returns_empty() {
    let cmd = cmd_with_stdout("");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result.is_empty(), "empty stdout should produce empty extraction");
}

#[test]
fn none_stdout_returns_empty() {
    let cmd = CommandRow {
        id: "test-id".into(),
        session_id: "sess".into(),
        command_raw: "some-command".into(),
        command_binary: Some("some-command".into()),
        stdout: None,
        source: "human".into(),
        timestamp_start: 1000,
        ..Default::default()
    };
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(result.is_empty(), "None stdout should produce empty extraction");
}

#[test]
fn deduplicates_entities() {
    let cmd = cmd_with_stdout("/home/user/file.rs\n/home/user/file.rs\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    let file_count = result.entities.iter().filter(|e| e.entity_type == "file").count();
    assert_eq!(file_count, 1, "duplicate file paths should be deduplicated");
}

#[test]
fn filters_dev_null() {
    let cmd = cmd_with_stdout("redirect to /dev/null\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.canonical_key.contains("/dev/null")),
        "/dev/null should be filtered out"
    );
}

#[test]
fn filters_dev_tty() {
    let cmd = cmd_with_stdout("opening /dev/tty for input\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        !result.entities.iter().any(|e| e.canonical_key.contains("/dev/tty")),
        "/dev/tty should be filtered out"
    );
}

#[test]
fn handles_ansi_escape_codes() {
    // File path wrapped in ANSI color codes
    let cmd = cmd_with_stdout("\x1b[31merror in /home/user/project/src/main.rs:10\x1b[0m\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    assert!(
        result.entities.iter().any(|e| e.entity_type == "file" && e.canonical_key.contains("main.rs")),
        "should extract file path through ANSI codes, got: {:?}",
        result.entities.iter().map(|e| (&e.entity_type, &e.canonical_key)).collect::<Vec<_>>()
    );
}

#[test]
fn does_not_extract_url_as_file_path() {
    let cmd = cmd_with_stdout("see https://example.com/path/to/thing\n");
    let ext = GenericExtractor;
    let result = ext.extract(&cmd).unwrap();
    // The /path/to/thing portion should not show up as a separate "file" entity
    // (it's part of a URL)
    let file_entities: Vec<_> = result.entities.iter()
        .filter(|e| e.entity_type == "file")
        .collect();
    assert!(
        file_entities.is_empty(),
        "URL path components should not be extracted as file paths, got: {:?}",
        file_entities.iter().map(|e| &e.canonical_key).collect::<Vec<_>>()
    );
}
