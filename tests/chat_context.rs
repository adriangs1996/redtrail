use redtrail::backend::knowledge::KnowledgeBase;
use redtrail::types::{ExecMode, Target};
use redtrail::workflows::chat::context::gather_context;

#[test]
fn gather_context_includes_target() {
    let kb = KnowledgeBase::default();
    let target = Target {
        base_url: None,
        hosts: vec!["10.10.10.1".into()],
        exec_mode: ExecMode::Local,
        auth_token: None,
        scope: vec![],
    };
    let ctx = gather_context(&kb, &target, "test-session");
    assert!(ctx.contains("10.10.10.1"));
    assert!(ctx.contains("test-session"));
}

#[test]
fn gather_context_includes_hosts() {
    let mut kb = KnowledgeBase::default();
    kb.discovered_hosts.push(redtrail::backend::knowledge::types::HostInfo {
        ip: "10.10.10.1".into(),
        ports: vec![],
        services: vec![],
        os: None,
    });
    let target = Target {
        base_url: None,
        hosts: vec!["10.10.10.1".into()],
        exec_mode: ExecMode::Local,
        auth_token: None,
        scope: vec![],
    };
    let ctx = gather_context(&kb, &target, "test-session");
    assert!(ctx.contains("10.10.10.1"));
}
