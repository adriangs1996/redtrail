use crate::config::Config;
use rusqlite::Connection;

pub struct AppContext {
    pub conn: Connection,
    pub config: Config,
    pub session_id: String,
}
