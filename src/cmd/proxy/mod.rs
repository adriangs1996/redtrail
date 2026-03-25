mod pty;

use redtrail::context::AppContext;
use redtrail::error::Error;

pub struct ProxyArgs {
    pub command: Vec<String>,
}

pub fn run(_ctx: &AppContext, _args: &ProxyArgs) -> Result<(), Error> {
    todo!()
}
