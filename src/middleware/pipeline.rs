use crate::error::Error;
use crate::workflows::session::SessionContext;
use crate::workflows::{CommandOutput, ShellCommand};
use std::sync::Arc;

pub trait Transform: Send + Sync {
    fn pre_exec(&self, cmd: &mut ShellCommand, ctx: &SessionContext) -> Result<(), Error>;
    fn post_exec(&self, output: &mut CommandOutput, ctx: &SessionContext) -> Result<(), Error>;
}

pub struct Pipeline {
    transforms: Vec<Arc<dyn Transform>>,
}

impl Pipeline {
    pub fn new() -> Self {
        Self { transforms: vec![] }
    }

    pub fn add(&mut self, t: Arc<dyn Transform>) {
        self.transforms.push(t);
    }

    pub fn run_pre_exec(
        &self,
        cmd: &mut ShellCommand,
        ctx: &SessionContext,
    ) -> Result<(), Error> {
        for t in &self.transforms {
            t.pre_exec(cmd, ctx)?;
        }
        Ok(())
    }

    pub fn run_post_exec(
        &self,
        output: &mut CommandOutput,
        ctx: &SessionContext,
    ) -> Result<(), Error> {
        for t in &self.transforms {
            t.post_exec(output, ctx)?;
        }
        Ok(())
    }
}

impl Default for Pipeline {
    fn default() -> Self {
        Self::new()
    }
}
