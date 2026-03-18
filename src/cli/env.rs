use crate::config::Config;
use crate::db::Db;
use crate::error::Error;
use crate::workspace;

pub fn run() -> Result<(), Error> {
    let cwd = std::env::current_dir()?;
    let ws = workspace::find_workspace(&cwd).ok_or(Error::NoWorkspace)?;
    let config = Config::resolved(&ws)?;
    let db = Db::open(workspace::db_path(&ws).to_str().unwrap())?;

    let (session_name, target) = db.conn().query_row(
        "SELECT name, COALESCE(target, '') FROM sessions LIMIT 1",
        [],
        |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)),
    ).map_err(|_| Error::NoActiveSession)?;

    let aliases = &config.tools.aliases;

    for tool in aliases {
        println!("alias {tool}='rt {tool}';");
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
    println!("export PS1=\"$RT_OLD_PS1\";");
    println!("unset RT_WORKSPACE RT_SESSION RT_TARGET RT_OLD_PS1;");
    println!("unset -f rt_deactivate;");
    Ok(())
}
