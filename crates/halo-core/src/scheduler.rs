use std::collections::VecDeque;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScheduledRun {
    pub agent_id: String,
    pub task_id: String,
}

impl ScheduledRun {
    fn new(agent_id: impl Into<String>, task_id: impl Into<String>) -> Self {
        Self {
            agent_id: agent_id.into(),
            task_id: task_id.into(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RunScheduler {
    max_global_runs: usize,
    max_per_agent_runs: usize,
    queued: VecDeque<ScheduledRun>,
    running: Vec<ScheduledRun>,
}

impl RunScheduler {
    pub fn new(max_global_runs: usize, max_per_agent_runs: usize) -> Self {
        Self {
            max_global_runs,
            max_per_agent_runs,
            queued: VecDeque::new(),
            running: Vec::new(),
        }
    }

    pub fn enqueue(&mut self, agent_id: impl Into<String>, task_id: impl Into<String>) {
        self.queued.push_back(ScheduledRun::new(agent_id, task_id));
    }

    pub fn start_ready(&mut self) -> Vec<ScheduledRun> {
        let mut started = Vec::new();
        let mut deferred = VecDeque::new();

        while let Some(candidate) = self.queued.pop_front() {
            if self.can_start(&candidate) {
                self.running.push(candidate.clone());
                started.push(candidate);
            } else {
                deferred.push_back(candidate);
            }
        }

        self.queued = deferred;
        started
    }

    pub fn finish(&mut self, task_id: &str) -> Option<ScheduledRun> {
        let index = self
            .running
            .iter()
            .position(|run| run.task_id.as_str() == task_id)?;

        Some(self.running.remove(index))
    }

    pub fn running_count(&self) -> usize {
        self.running.len()
    }

    pub fn queued_count(&self) -> usize {
        self.queued.len()
    }

    fn can_start(&self, candidate: &ScheduledRun) -> bool {
        self.running.len() < self.max_global_runs
            && self.running_for_agent(&candidate.agent_id) < self.max_per_agent_runs
    }

    fn running_for_agent(&self, agent_id: &str) -> usize {
        self.running
            .iter()
            .filter(|run| run.agent_id == agent_id)
            .count()
    }
}
