mod parse;
mod execute;
mod format;
pub mod highlight;
pub mod suggest;

use crate::db_v2::DbV2;
use crate::error::Error;

pub struct QueryInput {
    pub raw: String,
}

#[derive(Debug)]
pub struct QueryOutput {
    pub formatted: String,
    pub row_count: usize,
}

pub struct QueryWorkflow;

impl Default for QueryWorkflow {
    fn default() -> Self {
        Self::new()
    }
}

impl QueryWorkflow {
    pub fn new() -> Self { Self }

    pub fn run(&self, db: &DbV2, input: QueryInput) -> Result<QueryOutput, Error> {
        let sql = parse::validate(&input.raw)?;
        let result = execute::run(db, &sql)?;
        let formatted = format::as_table(&result);
        let row_count = result.rows.len();
        Ok(QueryOutput { formatted, row_count })
    }

    pub fn run_raw(&self, db: &DbV2, input: QueryInput) -> Result<(Vec<String>, Vec<Vec<String>>), Error> {
        let sql = parse::validate(&input.raw)?;
        let result = execute::run(db, &sql)?;
        Ok((result.columns, result.rows))
    }
}
