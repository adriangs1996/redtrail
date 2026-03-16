use redtrail::middleware::pipeline::{Pipeline, Transform};
use redtrail::workflows::{ShellCommand, CommandOutput};
use redtrail::workflows::session::SessionContext;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

struct PrefixTransform {
    prefix: String,
}

impl Transform for PrefixTransform {
    fn pre_exec(&self, cmd: &mut ShellCommand, _ctx: &SessionContext) -> Result<(), redtrail::Error> {
        cmd.raw = format!("{} {}", self.prefix, cmd.raw);
        Ok(())
    }

    fn post_exec(&self, _output: &mut CommandOutput, _ctx: &SessionContext) -> Result<(), redtrail::Error> {
        Ok(())
    }
}

#[test]
fn pipeline_runs_transforms_in_order() {
    let mut pipeline = Pipeline::new();
    pipeline.add(Arc::new(PrefixTransform { prefix: "A".into() }));
    pipeline.add(Arc::new(PrefixTransform { prefix: "B".into() }));

    let mut cmd = ShellCommand {
        raw: "cmd".into(),
        program: "cmd".into(),
        args: vec![],
        env_overrides: HashMap::new(),
        working_dir: PathBuf::from("."),
    };

    let ctx = SessionContext::new("test".into());
    pipeline.run_pre_exec(&mut cmd, &ctx).unwrap();

    assert_eq!(cmd.raw, "B A cmd");
}

#[test]
fn empty_pipeline_is_noop() {
    let pipeline = Pipeline::new();
    let mut cmd = ShellCommand {
        raw: "cmd".into(),
        program: "cmd".into(),
        args: vec![],
        env_overrides: HashMap::new(),
        working_dir: PathBuf::from("."),
    };
    let ctx = SessionContext::new("test".into());
    pipeline.run_pre_exec(&mut cmd, &ctx).unwrap();
    assert_eq!(cmd.raw, "cmd");
}
