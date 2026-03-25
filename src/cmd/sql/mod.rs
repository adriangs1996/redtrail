use redtrail::context::AppContext;
use redtrail::error::Error;

pub struct SqlArgs {
    pub query: String,
    pub json: bool,
}

pub fn run(_ctx: &AppContext, _args: &SqlArgs) -> Result<(), Error> {
    todo!()
}
