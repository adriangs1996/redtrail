use redtrail::completion::{CompletionEngine, CompletionResult};

#[test]
fn completes_builtins() {
    let engine = CompletionEngine::new();
    let results = engine.complete("sess", None);
    assert!(results.iter().any(|r| r.value == "session" && r.source == "builtin"));
}

#[test]
fn completes_builtin_subcommands() {
    let engine = CompletionEngine::new();
    let results = engine.complete("session sw", None);
    assert!(results.iter().any(|r| r.value == "switch"));
}

#[test]
fn no_results_for_random_prefix() {
    let engine = CompletionEngine::new();
    let results = engine.complete("zzzzz", None);
    assert!(results.is_empty());
}

#[test]
fn completes_all_builtins_on_empty() {
    let engine = CompletionEngine::new();
    let results = engine.complete("", None);
    assert!(results.len() >= 7);
}
