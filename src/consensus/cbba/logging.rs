use super::{CBBA, ScoreFunction};
use crate::consensus::types::{Agent, AgentId, BidInfo, ReleasedTasks, Task, TaskId};
use std::collections::HashMap;
use std::fmt::Display;

#[macro_export]
macro_rules! cbba_info {
    ($($arg:tt)+) => {
        tracing::info!(target: "cbba", $($arg)+)
    }
}

/// Log the tasks dropped by an agent during consensus
pub fn log_dropped_tasks<T, A, C>(cbba: &CBBA<T, A, C>, dropped: &ReleasedTasks)
where
    T: Task,
    A: Agent + Display,
    C: ScoreFunction<T, A>,
{
    if !dropped.is_empty() {
        for (task_id, successors) in dropped.iter() {
            if !successors.is_empty() {
                cbba_info!(
                    "  {} released task {} and its successors: {:?}",
                    cbba.agent,
                    task_id,
                    successors
                );
            } else {
                cbba_info!("  {} released task {}", cbba.agent, task_id);
            }
        }
    }
}

/// Log the status of all agents at the end of an iteration
pub fn log_iteration_status<T, A, C>(cbbas: &[CBBA<T, A, C>], iteration: usize)
where
    T: Task + Display,
    A: Agent + Display,
    C: ScoreFunction<T, A>,
{
    cbba_info!("End of Iteration {} Status:", iteration);
    for cbba in cbbas {
        let agent = &cbba.agent;
        let bundle = cbba.bundle.as_slice();
        let path = cbba.path.as_slice();
        let total_score: f64 = cbba
            .bids
            .values()
            .map(|info| match info {
                BidInfo::Winner(_, s, _) => s.val() as f64,
                BidInfo::None(_) => 0.0,
            })
            .sum();

        let bundle_str = bundle
            .iter()
            .map(|t| format!("{}", t))
            .collect::<Vec<_>>()
            .join(", ");
        let path_str = path
            .iter()
            .map(|t| format!("{}", t))
            .collect::<Vec<_>>()
            .join(", ");

        cbba_info!("  {} Bundle: [{}]", agent, bundle_str);
        cbba_info!("  {} Path: [{}]", agent, path_str);
        cbba_info!("    System Total Bid (Agent View): {:.2}", total_score);
    }
}

/// Log the final bundles and paths after consensus
pub fn log_final_status<T, A, C>(cbbas: &[CBBA<T, A, C>])
where
    T: Task + Display,
    A: Agent + Display,
    C: ScoreFunction<T, A>,
{
    cbba_info!("\nFinal bundles and paths after consensus:");
    for cbba in cbbas {
        let agent = &cbba.agent;
        let bundle_tasks = cbba.bundle.as_slice();
        let path_tasks = cbba.path.as_slice();
        let total_score: f64 = cbba
            .bids
            .values()
            .map(|info| match info {
                BidInfo::Winner(_, s, _) => s.val() as f64,
                BidInfo::None(_) => 0.0,
            })
            .sum();

        if !bundle_tasks.is_empty() {
            let bundle_str = bundle_tasks
                .iter()
                .map(|t| format!("{}", t))
                .collect::<Vec<_>>()
                .join(", ");
            let path_str = path_tasks
                .iter()
                .map(|t| format!("{}", t))
                .collect::<Vec<_>>()
                .join(", ");
            cbba_info!("{} bundle: [{}]", agent, bundle_str);
            cbba_info!("{} path: [{}]", agent, path_str);
        } else {
            cbba_info!("{} has no bundle tasks", agent);
        }
        cbba_info!("  System Total Bid (Agent View): {:.2}", total_score);
    }
}

/// Log the final task assignment table
pub fn log_assignment_table<T, A, C>(cbbas: &[CBBA<T, A, C>], tasks: &[T])
where
    T: Task,
    A: Agent,
    C: ScoreFunction<T, A>,
{
    cbba_info!("\nFinal Task Assignment Table:");
    let mut task_assignments: HashMap<TaskId, Option<AgentId>> =
        tasks.iter().map(|t| (t.id(), None)).collect();

    for cbba in cbbas {
        for task in &cbba.bundle {
            task_assignments.insert(task.id(), Some(cbba.agent.id()));
        }
    }

    let mut sorted_tasks: Vec<_> = task_assignments.into_iter().collect();
    sorted_tasks.sort_by_key(|(tid, _)| *tid);

    for (task_id, assigned_agent) in sorted_tasks {
        if let Some(agent_id) = assigned_agent {
            cbba_info!("T{}: A{}", task_id, agent_id);
        } else {
            cbba_info!("T{}: None", task_id);
        }
    }
}
