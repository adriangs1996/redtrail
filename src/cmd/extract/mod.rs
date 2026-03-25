mod extraction;

use redtrail::context::AppContext;
use redtrail::error::Error;

pub struct ExtractArgs {
    pub event_id: i64,
    pub force: bool,
}

pub fn run(_ctx: &AppContext, _args: &ExtractArgs) -> Result<(), Error> {
    todo!()
}
