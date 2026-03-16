use super::super::KnowledgeBase;
use super::super::types::command::CommandRecord;

impl KnowledgeBase {
    pub fn add_command(&mut self, record: CommandRecord) {
        self.command_history.push(record);
    }
}
