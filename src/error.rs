use std::fmt;

#[derive(Debug)]
pub enum Error {
    Db(String),
    Config(String),
    Io(std::io::Error),
    NoWorkspace,
    NoActiveSession,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Db(e) => write!(f, "database error: {e}"),
            Error::Config(e) => write!(f, "config error: {e}"),
            Error::Io(e) => write!(f, "io error: {e}"),
            Error::NoWorkspace => write!(f, "no redtrail workspace found (run `rt init`)"),
            Error::NoActiveSession => write!(f, "no active session"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self { Error::Io(e) }
}
