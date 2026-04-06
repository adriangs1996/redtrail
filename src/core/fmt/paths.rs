/// Convert an absolute path to a relative path from the project root.
/// Returns the original path if it's not under the project root.
#[allow(clippy::needless_lifetimes)]
pub fn to_relative<'a>(path: &'a str, project_root: &str) -> &'a str {
    let root = project_root.trim_end_matches('/');
    if let Some(rest) = path.strip_prefix(root) {
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        if rest.is_empty() { "." } else { rest }
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_project_root() {
        assert_eq!(to_relative("/home/user/project/src/main.rs", "/home/user/project"), "src/main.rs");
    }

    #[test]
    fn root_with_trailing_slash() {
        assert_eq!(to_relative("/home/user/project/src/main.rs", "/home/user/project/"), "src/main.rs");
    }

    #[test]
    fn path_not_under_root() {
        assert_eq!(to_relative("/other/path/file.rs", "/home/user/project"), "/other/path/file.rs");
    }

    #[test]
    fn path_is_root() {
        assert_eq!(to_relative("/home/user/project", "/home/user/project"), ".");
    }

    #[test]
    fn already_relative() {
        assert_eq!(to_relative("src/main.rs", "/home/user/project"), "src/main.rs");
    }
}
