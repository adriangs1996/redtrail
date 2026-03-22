use crate::error::Error;
use crate::resolve;
use std::env;

pub fn run(target: Option<String>, goal: String, scope: Option<String>) -> Result<(), Error> {
    let cwd = env::current_dir()?;
    let ctx = resolve::resolve_global()?;

    if ctx.find_session(&cwd)?.is_some() {
        return Err(Error::Config(
            "session already active for this directory, use `rt session new`".into(),
        ));
    }

    let session_name = cwd
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("default")
        .to_string();
    let session_id = uuid::Uuid::new_v4().to_string();
    let workspace_path = cwd.to_string_lossy();

    crate::db::session::create_session(
        &ctx.conn,
        &session_id,
        &session_name,
        &workspace_path,
        target.as_deref(),
        scope.as_deref(),
        &goal,
    )?;

    println!("session created: {session_name}");
    println!("  workspace: {}", cwd.display());
    if let Some(ref t) = target {
        println!("  target: {t}");
    }
    println!("\nactivate with: eval \"$(rt env)\"");

    Ok(())
}
