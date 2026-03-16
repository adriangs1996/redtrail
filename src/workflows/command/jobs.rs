use std::collections::HashMap;
use std::time::Instant;

#[derive(Debug, Clone)]
pub struct Job {
    pub id: u32,
    pub command: String,
    pub block_id: usize,
    pub started_at: Instant,
    pub finished: bool,
    pub exit_code: Option<i32>,
}

pub struct JobTable {
    jobs: HashMap<u32, Job>,
    next_id: u32,
}

impl Default for JobTable {
    fn default() -> Self {
        Self::new()
    }
}

impl JobTable {
    pub fn new() -> Self {
        Self { jobs: HashMap::new(), next_id: 1 }
    }

    pub fn add(&mut self, command: String, block_id: usize) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        self.jobs.insert(id, Job {
            id,
            command,
            block_id,
            started_at: Instant::now(),
            finished: false,
            exit_code: None,
        });
        id
    }

    pub fn finish(&mut self, id: u32, exit_code: i32) {
        if let Some(job) = self.jobs.get_mut(&id) {
            job.finished = true;
            job.exit_code = Some(exit_code);
        }
    }

    pub fn get(&self, id: u32) -> Option<&Job> {
        self.jobs.get(&id)
    }

    pub fn list(&self) -> Vec<&Job> {
        let mut jobs: Vec<_> = self.jobs.values().collect();
        jobs.sort_by_key(|j| j.id);
        jobs
    }

    pub fn running_count(&self) -> usize {
        self.jobs.values().filter(|j| !j.finished).count()
    }

    pub fn remove_finished(&mut self) -> Vec<Job> {
        let finished_ids: Vec<u32> = self.jobs.values()
            .filter(|j| j.finished)
            .map(|j| j.id)
            .collect();
        finished_ids.iter().filter_map(|id| self.jobs.remove(id)).collect()
    }
}
