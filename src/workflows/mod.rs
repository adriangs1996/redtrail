pub mod chat;
pub mod command;
pub mod query;
pub mod session;
pub mod types;

pub use types::*;

use crate::error::Error;

pub trait Workflow {
    type Input;
    type Output;

    fn execute(
        &self,
        input: Self::Input,
    ) -> impl std::future::Future<Output = Result<Self::Output, Error>> + Send;
}
