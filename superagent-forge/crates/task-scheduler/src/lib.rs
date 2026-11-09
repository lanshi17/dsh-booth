//! DAG validation and bounded concurrent scheduling.
use forge_domain::TaskId;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub enum ScheduleError {
    #[error("dependency points to a task outside the submitted graph")]
    MissingDependency,
    #[error("task dependency graph contains a cycle")]
    Cycle,
    #[error("concurrency limit must be at least 1")]
    InvalidConcurrency,
    #[error("task is not in a runnable state")]
    InvalidTransition,
    #[error("unknown task in scheduler")]
    UnknownTask,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NodeState {
    Waiting,
    Ready,
    Running,
    Succeeded,
    Failed,
    /// Skipped because a dependency failed.
    Blocked,
}

/// Kahn ordering used for static validation and deterministic tests.
pub fn order(graph: &BTreeMap<TaskId, BTreeSet<TaskId>>) -> Result<Vec<TaskId>, ScheduleError> {
    if graph.values().flatten().any(|id| !graph.contains_key(id)) {
        return Err(ScheduleError::MissingDependency);
    }
    let mut pending = graph.clone();
    let mut sorted = Vec::with_capacity(graph.len());
    while !pending.is_empty() {
        let ready: Vec<_> = pending
            .iter()
            .filter(|(_, deps)| deps.is_empty())
            .map(|(id, _)| *id)
            .collect();
        if ready.is_empty() {
            return Err(ScheduleError::Cycle);
        }
        for id in ready {
            pending.remove(&id);
            for deps in pending.values_mut() {
                deps.remove(&id);
            }
            sorted.push(id);
        }
    }
    Ok(sorted)
}

/// Runtime queue that respects `max_concurrency` and dependency failure fan-out.
#[derive(Debug)]
pub struct ConcurrentScheduler {
    max_concurrency: usize,
    deps: BTreeMap<TaskId, BTreeSet<TaskId>>,
    state: BTreeMap<TaskId, NodeState>,
}

impl ConcurrentScheduler {
    pub fn new(
        graph: BTreeMap<TaskId, BTreeSet<TaskId>>,
        max_concurrency: usize,
    ) -> Result<Self, ScheduleError> {
        if max_concurrency == 0 {
            return Err(ScheduleError::InvalidConcurrency);
        }
        // Reject cycles / missing deps up front.
        let _ = order(&graph)?;
        let mut state = BTreeMap::new();
        for (id, deps) in &graph {
            let initial = if deps.is_empty() {
                NodeState::Ready
            } else {
                NodeState::Waiting
            };
            state.insert(*id, initial);
        }
        Ok(Self {
            max_concurrency,
            deps: graph,
            state,
        })
    }

    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    pub fn state(&self, id: TaskId) -> Option<NodeState> {
        self.state.get(&id).copied()
    }

    pub fn running_count(&self) -> usize {
        self.state
            .values()
            .filter(|s| **s == NodeState::Running)
            .count()
    }

    /// Returns up to remaining concurrency slots of Ready tasks and marks them Running.
    pub fn claim_ready(&mut self) -> Vec<TaskId> {
        let slots = self.max_concurrency.saturating_sub(self.running_count());
        if slots == 0 {
            return Vec::new();
        }
        let ready: Vec<TaskId> = self
            .state
            .iter()
            .filter(|(_, s)| **s == NodeState::Ready)
            .map(|(id, _)| *id)
            .take(slots)
            .collect();
        for id in &ready {
            self.state.insert(*id, NodeState::Running);
        }
        ready
    }

    pub fn mark_succeeded(&mut self, id: TaskId) -> Result<(), ScheduleError> {
        self.finish(id, NodeState::Succeeded)
    }

    pub fn mark_failed(&mut self, id: TaskId) -> Result<(), ScheduleError> {
        self.finish(id, NodeState::Failed)?;
        self.block_dependents(id);
        Ok(())
    }

    pub fn is_complete(&self) -> bool {
        self.state.values().all(|s| {
            matches!(
                s,
                NodeState::Succeeded | NodeState::Failed | NodeState::Blocked
            )
        })
    }

    fn finish(&mut self, id: TaskId, next: NodeState) -> Result<(), ScheduleError> {
        match self.state.get(&id).copied() {
            Some(NodeState::Running) => {
                self.state.insert(id, next);
                if next == NodeState::Succeeded {
                    self.unlock_dependents(id);
                }
                Ok(())
            }
            Some(_) => Err(ScheduleError::InvalidTransition),
            None => Err(ScheduleError::UnknownTask),
        }
    }

    fn unlock_dependents(&mut self, completed: TaskId) {
        let dependents: Vec<TaskId> = self
            .deps
            .iter()
            .filter(|(_, deps)| deps.contains(&completed))
            .map(|(id, _)| *id)
            .collect();
        for id in dependents {
            if self.state.get(&id) != Some(&NodeState::Waiting) {
                continue;
            }
            let deps = &self.deps[&id];
            let ready = deps
                .iter()
                .all(|dep| matches!(self.state.get(dep), Some(NodeState::Succeeded)));
            if ready {
                self.state.insert(id, NodeState::Ready);
            }
        }
    }

    fn block_dependents(&mut self, failed: TaskId) {
        let mut stack = vec![failed];
        let mut seen = BTreeSet::new();
        while let Some(current) = stack.pop() {
            if !seen.insert(current) {
                continue;
            }
            let children: Vec<TaskId> = self
                .deps
                .iter()
                .filter(|(_, deps)| deps.contains(&current))
                .map(|(id, _)| *id)
                .collect();
            for child in children {
                if matches!(
                    self.state.get(&child),
                    Some(NodeState::Waiting | NodeState::Ready)
                ) {
                    self.state.insert(child, NodeState::Blocked);
                    stack.push(child);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dependencies_precede_dependents() {
        let a = TaskId::new();
        let b = TaskId::new();
        let c = TaskId::new();
        let graph = BTreeMap::from([
            (a, BTreeSet::new()),
            (b, BTreeSet::from([a])),
            (c, BTreeSet::from([a, b])),
        ]);
        assert_eq!(order(&graph).unwrap(), vec![a, b, c]);
    }

    #[test]
    fn rejects_cycles_and_dangling_dependencies() {
        let a = TaskId::new();
        let b = TaskId::new();
        assert_eq!(
            order(&BTreeMap::from([(a, BTreeSet::from([b]))])),
            Err(ScheduleError::MissingDependency)
        );
        assert_eq!(
            order(&BTreeMap::from([
                (a, BTreeSet::from([b])),
                (b, BTreeSet::from([a]))
            ])),
            Err(ScheduleError::Cycle)
        );
    }

    #[test]
    fn concurrency_limit_and_dependency_failure() {
        let a = TaskId::new();
        let b = TaskId::new();
        let c = TaskId::new();
        let graph = BTreeMap::from([
            (a, BTreeSet::new()),
            (b, BTreeSet::new()),
            (c, BTreeSet::from([a])),
        ]);
        let mut sched = ConcurrentScheduler::new(graph, 1).unwrap();
        let first = sched.claim_ready();
        assert_eq!(first.len(), 1);
        assert_eq!(sched.running_count(), 1);
        assert!(sched.claim_ready().is_empty(), "concurrency capped at 1");

        let running = first[0];
        if running == a {
            sched.mark_failed(a).unwrap();
            assert_eq!(sched.state(c), Some(NodeState::Blocked));
            let next = sched.claim_ready();
            assert_eq!(next, vec![b]);
            sched.mark_succeeded(b).unwrap();
        } else {
            sched.mark_succeeded(b).unwrap();
            let next = sched.claim_ready();
            assert_eq!(next, vec![a]);
            sched.mark_failed(a).unwrap();
            assert_eq!(sched.state(c), Some(NodeState::Blocked));
        }
        assert!(sched.is_complete());
    }
}
