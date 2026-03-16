use redtrail::tui::prompt::render_prompt;
use std::collections::HashMap;

#[test]
fn renders_template_variables() {
    let mut vars = HashMap::new();
    vars.insert("session".to_string(), "htb-box".to_string());
    vars.insert("status".to_string(), "✓".to_string());

    let result = render_prompt("redtrail:{session} {status}$ ", &vars);
    assert_eq!(result, "redtrail:htb-box ✓$ ");
}

#[test]
fn unknown_variables_left_as_is() {
    let vars = HashMap::new();
    let result = render_prompt("{unknown}$ ", &vars);
    assert_eq!(result, "{unknown}$ ");
}

#[test]
fn empty_template() {
    let vars = HashMap::new();
    let result = render_prompt("", &vars);
    assert_eq!(result, "");
}
