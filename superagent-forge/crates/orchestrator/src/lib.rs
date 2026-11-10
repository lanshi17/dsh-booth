//! Composition root: trusted use cases, DAG concurrency, optional executors.
use forge_agent_backend::AgentExecutor;
use forge_application::TaskService;
use forge_domain::TaskId;
use forge_task_scheduler::ConcurrentScheduler;
use forge_task_scheduler::NodeState;
use forge_task_scheduler::ScheduleError;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

pub struct TaskOrchestrator {
    pub tasks: TaskService,
    pub executors: Vec<Box<dyn AgentExecutor>>,
}

/// Drive a dependency graph under a concurrency cap using caller-provided work.
pub struct DagRunner {
    scheduler: ConcurrentScheduler,
}

impl DagRunner {
    pub fn new(
        graph: BTreeMap<TaskId, BTreeSet<TaskId>>,
        max_concurrency: usize,
    ) -> Result<Self, ScheduleError> {
        Ok(Self {
            scheduler: ConcurrentScheduler::new(graph, max_concurrency)?,
        })
    }

    pub fn scheduler(&self) -> &ConcurrentScheduler {
        &self.scheduler
    }

    /// Run until the graph is complete. `work` returns Ok for success, Err for failure.
    pub fn run_to_completion<F>(&mut self, mut work: F) -> Result<(), ScheduleError>
    where
        F: FnMut(TaskId) -> Result<(), ()>,
    {
        while !self.scheduler.is_complete() {
            let batch = self.scheduler.claim_ready();
            if batch.is_empty() {
                // No runnable work but not complete ⇒ should not happen after validation.
                if self.scheduler.running_count() == 0 {
                    break;
                }
                return Err(ScheduleError::InvalidTransition);
            }
            for id in batch {
                match work(id) {
                    Ok(()) => self.scheduler.mark_succeeded(id)?,
                    Err(()) => self.scheduler.mark_failed(id)?,
                }
            }
        }
        Ok(())
    }

    pub fn state(&self, id: TaskId) -> Option<NodeState> {
        self.scheduler.state(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runner_blocks_dependents_when_dependency_fails() {
        let a = TaskId::new();
        let b = TaskId::new();
        let c = TaskId::new();
        let graph = BTreeMap::from([
            (a, BTreeSet::new()),
            (b, BTreeSet::new()),
            (c, BTreeSet::from([a])),
        ]);
        let mut runner = DagRunner::new(graph, 2).unwrap();
        assert_eq!(runner.scheduler().max_concurrency(), 2);
        runner
            .run_to_completion(|id| if id == a { Err(()) } else { Ok(()) })
            .unwrap();
        assert_eq!(runner.state(c), Some(NodeState::Blocked));
        assert_eq!(runner.state(a), Some(NodeState::Failed));
        assert_eq!(runner.state(b), Some(NodeState::Succeeded));
    }
}
