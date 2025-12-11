use crate::cbba::ScoreFunction;
use crate::consensus::types::{Agent, Task};
use crate::CBBA;
use serde::Serialize;
use std::collections::HashMap;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::time::Instant;

#[derive(Debug, Serialize)]
pub struct TaskReleaseRecord {
    pub task_id: u32,
    pub successors: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub struct AgentIterationLog {
    pub agent_id: u32,
    pub added_tasks: Vec<u32>,
    pub released_tasks: Vec<TaskReleaseRecord>,
    pub bundle: Vec<u32>,
    pub path: Vec<u32>,
}

#[derive(Debug, Serialize)]
pub struct IterationLog {
    pub iteration: usize,
    pub bundle_stage_ms: f64,
    pub consensus_stage_ms: f64,
    pub iteration_ms: f64,
    pub converged_after_this: bool,
    pub agents: Vec<AgentIterationLog>,
}

#[derive(Debug, Serialize)]
pub struct FinalAssignment {
    pub task_id: u32,
    pub agent_id: Option<u32>,
}

#[derive(Debug, Serialize)]
pub struct SimulationReport {
    pub iterations: Vec<IterationLog>,
    pub final_assignments: Vec<FinalAssignment>,
    pub total_duration_ms: f64,
    pub time_per_iteration_ms: f64,
    pub agent_count: usize,
    pub task_count: usize,
}

pub fn build_report<T, A, C>(
    cbbas: &[CBBA<T, A, C>],
    tasks: &[T],
    iteration_logs: Vec<IterationLog>,
    sim_start: Instant,
) -> SimulationReport
where
    T: Task,
    A: Agent,
    C: ScoreFunction<T, A>,
{
    let mut task_assignments: HashMap<u32, Option<u32>> =
        tasks.iter().map(|t| (t.id().0, None)).collect();

    for cbba in cbbas {
        for task in cbba.bundle.iter() {
            task_assignments.insert(task.id().0, Some(cbba.agent.id().0));
        }
    }

    let mut final_assignments: Vec<_> = task_assignments
        .into_iter()
        .map(|(task_id, agent_id)| FinalAssignment { task_id, agent_id })
        .collect();
    final_assignments.sort_by_key(|a| a.task_id);

    let total_duration_ms = sim_start.elapsed().as_secs_f64() * 1000.0;
    let time_per_iteration_ms = if iteration_logs.is_empty() {
        0.0
    } else {
        total_duration_ms / (iteration_logs.len() as f64)
    };
    let agent_count = cbbas.len();
    let task_count = tasks.len();

    SimulationReport {
        iterations: iteration_logs,
        final_assignments,
        total_duration_ms,
        time_per_iteration_ms,
        agent_count,
        task_count,
    }
}

pub fn write_report_json<P: AsRef<Path>>(
    report: &SimulationReport,
    result_dir: P,
) -> std::io::Result<PathBuf> {
    let summary_path = result_dir.as_ref().join("summary.json");
    let mut summary_file = File::create(&summary_path)?;
    serde_json::to_writer_pretty(&mut summary_file, report)?;
    Ok(summary_path)
}
