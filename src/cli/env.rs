use crate::config::Config;
use crate::db::SessionOps;
use crate::error::Error;
use crate::workspace;

const COMMAND_ALIASES: &[(&str, &str)] = &[
    ("kb",     "kb"),
    ("st",     "status"),
    ("theory", "hypothesis"),
    ("ev",     "evidence"),
    ("sess",   "session"),
    ("scope",  "scope"),
    ("conf",   "config"),
    ("eat",    "ingest"),
    ("rep",    "report"),
    ("skill",  "skill"),
    ("ask",    "ask"),
    ("q",      "query"),
    ("sql",    "sql"),
];

pub fn run(db: &impl SessionOps, session_id: &str) -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let config = Config::resolved(&ws)?;

    let session = db.get_session(session_id)?;
    let session_name = session["name"].as_str().unwrap_or("").to_string();
    let target = session["target"].as_str().unwrap_or("").to_string();

    let aliases = &config.tools.aliases;

    for tool in aliases {
        println!("alias {tool}='rt {tool}';");
    }

    for (short, full) in COMMAND_ALIASES {
        println!("alias {short}='rt {full}';");
    }

    println!("export RT_WORKSPACE='{}';", ws.display());
    println!("export RT_SESSION='{session_name}';");
    if !target.is_empty() {
        println!("export RT_TARGET='{target}';");
    }

    println!("export RT_OLD_PS1=\"$PS1\";");
    println!("export PS1=\"[rt:{session_name}] $PS1\";");

    print!("rt_deactivate() {{ ");
    for tool in aliases {
        print!("unalias {tool} 2>/dev/null; ");
    }
    for (short, _) in COMMAND_ALIASES {
        print!("unalias {short} 2>/dev/null; ");
    }
    print!("export PS1=\"$RT_OLD_PS1\"; ");
    print!("unset RT_WORKSPACE RT_SESSION RT_TARGET RT_OLD_PS1; ");
    print!("unset -f rt_deactivate; ");
    println!("}};");

    Ok(())
}

pub fn deactivate() -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let config = Config::resolved(&ws)?;

    for tool in &config.tools.aliases {
        println!("unalias {tool} 2>/dev/null;");
    }
    for (short, _) in COMMAND_ALIASES {
        println!("unalias {short} 2>/dev/null;");
    }
    println!("export PS1=\"$RT_OLD_PS1\";");
    println!("unset RT_WORKSPACE RT_SESSION RT_TARGET RT_OLD_PS1;");
    println!("unset -f rt_deactivate;");
    Ok(())
}
