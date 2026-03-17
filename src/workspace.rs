use std::path::{Path, PathBuf};

pub fn find_workspace(start: &Path) -> Option<PathBuf> {
    let mut dir = start.to_path_buf();
    loop {
        if dir.join(".redtrail").is_dir() {
            return Some(dir);
        }
        if !dir.pop() {
            return None;
        }
    }
}

pub fn db_path(workspace: &Path) -> PathBuf {
    workspace.join(".redtrail/redtrail.db")
}

pub fn config_path(workspace: &Path) -> PathBuf {
    workspace.join(".redtrail/config.toml")
}

pub fn aliases_path(workspace: &Path) -> PathBuf {
    workspace.join(".redtrail/aliases.sh")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_find_workspace_in_current_dir() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".redtrail")).unwrap();
        let ws = find_workspace(tmp.path()).unwrap();
        assert_eq!(ws, tmp.path());
    }

    #[test]
    fn test_find_workspace_walks_up() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join(".redtrail")).unwrap();
        let child = tmp.path().join("subdir/deep");
        fs::create_dir_all(&child).unwrap();
        let ws = find_workspace(&child).unwrap();
        assert_eq!(ws, tmp.path());
    }

    #[test]
    fn test_no_workspace_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(find_workspace(tmp.path()).is_none());
    }

    #[test]
    fn test_path_helpers() {
        let ws = Path::new("/home/user/ctf");
        assert_eq!(db_path(ws), PathBuf::from("/home/user/ctf/.redtrail/redtrail.db"));
        assert_eq!(config_path(ws), PathBuf::from("/home/user/ctf/.redtrail/config.toml"));
        assert_eq!(aliases_path(ws), PathBuf::from("/home/user/ctf/.redtrail/aliases.sh"));
    }
}
