//! Consensus-Based Bundle Algorithm (CBBA) implementation

pub mod logging;

pub use logging::*;

use crate::cbba_info;
use crate::consensus::types::{
    AddedTasks, Agent, AgentId, BidInfo, BoundedSortedSet, ConsensusMessage, ReleasedTasks, Score,
    Task, TaskId,
};
use crate::error::Error;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Configuration for CBBA algorithm
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
    /// Maximum number of tasks per agent (Lt in paper)
    /// If set to 0, it means unbounded (no limit)
    pub max_tasks_per_agent: usize,
}

impl Config {
    /// Set the maximum number of tasks per agent
    pub fn with_max_tasks_per_agent(mut self, max_tasks: usize) -> Self {
        self.max_tasks_per_agent = max_tasks;
        self
    }
}

/// Trait for scoring functions that evaluate task assignments for CBBA
///
/// Score functions determine how valuable a task is for an agent. Higher
/// values indicate better assignments (the algorithms maximize these scores).
pub trait ScoreFunction<T: Task, A: Agent> {
    /// Check if the task can be validly assigned to the agent
    ///
    /// Constraints determine whether a task CAN be assigned to an agent.
    /// If check returns false, the task cannot be added regardless of score.
    ///
    /// # Returns
    /// true if assignment is valid, false otherwise
    fn check(&self, _agent: &A, _task: &T) -> bool {
        true
    }

    /// Calculate the scores for a sequence of tasks
    ///
    /// This method calculates the score for each task in the provided path,
    /// considering the order of execution.
    ///
    /// # Returns
    ///
    /// A vector of scores corresponding to each task in the path.
    fn calc_path(&self, agent: &A, path: &[T]) -> Vec<Score>;

    /// Calculate the total score for a sequence of tasks
    ///
    /// This method sums up the scores returned by `calc_path`.
    ///
    /// # Returns
    ///
    /// The total score for the path.
    fn calc_total_score(&self, agent: &A, path: &[T]) -> Score {
        let scores = self.calc_path(agent, path);
        Score::new(scores.iter().map(|s| s.val()).sum())
    }
}

/// Bundle for CBBA - ordered list of tasks
pub type Bundle<T> = BoundedSortedSet<T>;

/// Path representation for CBBA - tasks ordered by execution sequence
pub type Path<T> = BoundedSortedSet<T>;

/// Consensus-Based Bundle Algorithm (CBBA) implementation
///
/// CBBA extends CBAA to handle multi-assignment problems where each agent
/// can be assigned multiple tasks in sequence. It uses bundle construction
/// to build a sequence of tasks and more sophisticated conflict resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CBBA<T: Task, A: Agent, C: ScoreFunction<T, A>> {
    pub agent: A,
    pub config: Config,
    pub score_function: C,
    pub current_tasks: Vec<T>,
    pub bundle: Bundle<T>,
    pub path: Path<T>,

    /// Bids for each task
    pub bids: HashMap<TaskId, BidInfo>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Update,
    Reset,
    Leave,
}

impl<T: Task, A: Agent, C: ScoreFunction<T, A>> CBBA<T, A, C> {
    pub fn new(
        agent: A,
        score_function: C,
        available_tasks: Vec<T>,
        config: Option<Config>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let max_tasks = if config.max_tasks_per_agent == 0 {
            usize::MAX
        } else {
            config.max_tasks_per_agent
        };

        Self {
            agent,
            config,
            score_function,
            current_tasks: available_tasks,
            bundle: Bundle::new(max_tasks),
            path: Path::new(max_tasks),
            bids: HashMap::new(),
        }
    }

    /// Reset the internal state of the CBBA agent, clearing bundles, paths, and bids.
    pub fn clear(&mut self) {
        self.current_tasks.clear();
        self.bundle.clear();
        self.path.clear();
        self.bids.clear();
    }

    /// Phase 1: Bundle Construction
    ///
    /// Accepts an optional slice of tasks. If provided, uses these tasks and updates current_tasks.
    /// If not provided, uses current_tasks.
    pub fn bundle_construction_phase(&mut self, tasks: Option<&[T]>) -> Result<AddedTasks, Error> {
        if let Some(ts) = tasks {
            if ts.is_empty() {
                self.clear();
                return Ok(AddedTasks::default());
            }
            // Remove tasks not in the new list from bundle, path, and bids
            use std::collections::HashSet;
            let new_set: HashSet<_> = ts.iter().map(|t| t.id()).collect();
            let old_set: HashSet<_> = self.current_tasks.iter().map(|t| t.id()).collect();
            let removed: Vec<_> = old_set.difference(&new_set).cloned().collect();

            // Remove from bundle
            self.bundle.retain(|t| new_set.contains(&t.id()));
            // Remove from path
            self.path.retain(|t| new_set.contains(&t.id()));
            // Remove from bids
            for tid in &removed {
                self.bids.remove(tid);
            }

            self.current_tasks = ts.to_vec();
        }
        let task_source: &[T] = &self.current_tasks;
        let mut added_tasks = AddedTasks::default();

        while !self.bundle.is_full() {
            // Select task with highest marginal score improvement from the given task source
            let Some((selected_task, marginal_score, new_path)) =
                self.select_best_marginal_task_from(task_source)
            else {
                break;
            };

            match self.bundle.push(selected_task.clone()) {
                Ok(_) => {}
                Err(Error::CapacityFull) => {
                    panic!("Capacity full but bundle.is_full() returned false")
                }
                Err(e) => panic!("Failed to push to bundle: {:?}", e),
            }

            self.path = new_path;

            let task_id = selected_task.id();
            self.bids
                .insert(task_id, BidInfo::winner(self.agent.id(), marginal_score));

            cbba_info!(
                "Agent {} added task {} with score {}",
                self.agent.id(),
                task_id,
                marginal_score
            );

            added_tasks.push(task_id);
        }

        Ok(added_tasks)
    }

    /// Like select_best_marginal_task, but takes a task slice to search from
    fn select_best_marginal_task_from(
        &self,
        tasks: &[T],
    ) -> Option<(T, Score, BoundedSortedSet<T>)> {
        tasks
            .iter()
            .filter(|task| {
                !self.bundle.contains(task) && self.score_function.check(&self.agent, task)
            })
            .filter_map(|task| {
                let (marginal_score, new_path) = self.calculate_marginal_score(task)?;
                let current_winning_bid = match self.bids.get(&task.id()) {
                    Some(BidInfo::Winner(_, s, _)) => *s,
                    _ => Score::default(),
                };
                if marginal_score > current_winning_bid {
                    Some((task.clone(), marginal_score, new_path))
                } else {
                    None
                }
            })
            .max_by(|(_, score_a, _), (_, score_b, _)| score_a.cmp(score_b))
    }

    fn calculate_marginal_score(&self, task: &T) -> Option<(Score, BoundedSortedSet<T>)> {
        if self.bundle.contains(task) {
            panic!("Task already in bundle in calculate_marginal_score");
        }

        let current_total_score = self
            .score_function
            .calc_total_score(&self.agent, self.path.as_slice());
        let (_, new_total_score, best_path) = self.find_best_insert_position(&self.path, task)?;

        let marginal = new_total_score.saturating_sub(current_total_score);

        Some((marginal, best_path))
    }

    fn find_best_insert_position(
        &self,
        path: &BoundedSortedSet<T>,
        task: &T,
    ) -> Option<(usize, Score, BoundedSortedSet<T>)> {
        if path.is_full() {
            return None;
        }

        let mut best_position = 0;
        let mut best_score = Score::default();
        let mut best_path = path.clone();

        // Try inserting at each possible position
        for pos in 0..=path.len() {
            let mut temp_path = path.clone();
            if temp_path.insert(pos, task.clone()).is_ok() {
                // Calculate total score for this configuration
                let current_score = self
                    .score_function
                    .calc_total_score(&self.agent, temp_path.as_slice());

                if current_score > best_score {
                    best_score = current_score;
                    best_position = pos;
                    best_path = temp_path;
                }
            }
        }

        if best_score.is_zero() && !path.is_empty() {
            None
        } else {
            Some((best_position, best_score, best_path))
        }
    }

    /// Build a consensus message representing the current winning bids for this agent.
    pub fn consensus_message(&self) -> ConsensusMessage {
        ConsensusMessage {
            agent_id: self.agent.id(),
            bids: self.bids.clone(),
        }
    }

    /// Phase 2: Consensus
    ///
    /// This phase handles conflict resolution by processing incoming consensus messages.
    /// It updates the internal state (winning bids/agents) and releases tasks if outbid.
    pub fn consensus_phase(&mut self, messages: &[ConsensusMessage]) -> ReleasedTasks {
        let mut dropped_tasks = ReleasedTasks::default();
        // Implement consensus
        // In CBBA, this updates y_ij and z_ij based on messages
        for message in messages {
            // Skip messages from self
            if message.agent_id == self.agent.id() {
                continue;
            }

            let mut tasks_released_directly = Vec::new();

            let sender_k = message.agent_id;
            let self_i = self.agent.id();

            // Process all task IDs from both message and current winners
            let all_task_ids: std::collections::HashSet<_> = message
                .bids
                .keys()
                .chain(self.bids.keys())
                .cloned()
                .collect();

            for task_id in all_task_ids {
                let sender_bid = message
                    .bids
                    .get(&task_id)
                    .cloned()
                    .unwrap_or(BidInfo::none_initial());
                let receiver_bid = self
                    .bids
                    .get(&task_id)
                    .cloned()
                    .unwrap_or(BidInfo::none_initial());

                // Pre-calculate extracted values for logic after action determination
                let (y_kj, z_kj, t_kj) = match sender_bid {
                    BidInfo::Winner(w, s, t) => (s, Some(w), t),
                    BidInfo::None(t) => (Score::default(), None, t),
                };

                let action = determine_action(sender_k, self_i, &sender_bid, &receiver_bid);

                match action {
                    Action::Update => {
                        let new_info = if let Some(winner) = z_kj {
                            BidInfo::winner_with_timestamp(winner, y_kj, t_kj)
                        } else {
                            BidInfo::none_with_timestamp(t_kj)
                        };
                        self.bids.insert(task_id, new_info);
                    }
                    Action::Reset => {
                        self.bids
                            .insert(task_id, BidInfo::none_with_timestamp(t_kj));
                    }
                    Action::Leave => {}
                }

                // Check if we need to release this task (if we were the winner but now we are not)
                // This happens if we Updated to someone else, or Reset.
                let current_winner = match self.bids.get(&task_id) {
                    Some(BidInfo::Winner(w, _, _)) => Some(*w),
                    _ => None,
                };
                if current_winner != Some(self_i) {
                    if let Some(task) = self.bundle.find(|t| task_id == t.id()) {
                        cbba_info!(
                            "Agent {} releasing task {} due to conflict resolution",
                            self_i,
                            task_id
                        );
                        tasks_released_directly.push(task.clone());
                    }
                }
            }

            // Apply task releases
            for task in &tasks_released_directly {
                let tasks_released_indirectly = self.release_task_and_successors(task);
                dropped_tasks.insert(task.id(), tasks_released_indirectly);
            }
        }
        dropped_tasks
    }

    pub fn release_task_and_successors(&mut self, task: &T) -> Vec<TaskId> {
        // Find position of task in bundle
        let Some(pos) = self.bundle.iter().position(|t| task.id() == t.id()) else {
            return Vec::new();
        };

        // Remove this task and all tasks added after it (as per CBBA rules)
        let tasks_to_remove: Vec<T> = self.bundle.as_slice()[pos..].to_vec();

        if tasks_to_remove.len() > 1 {
            let successor_ids: Vec<_> = tasks_to_remove
                .iter()
                .skip(1)
                .map(|t| t.id().to_string())
                .collect();
            cbba_info!(
                "Agent {} removing successors of task {}: [{}]",
                self.agent.id(),
                task.id(),
                successor_ids.join(", ")
            );
        }

        // Truncate bundle directly
        self.bundle.truncate(pos);

        let mut removed_ids = Vec::new();

        for task_to_remove in &tasks_to_remove {
            removed_ids.push(task_to_remove.id());
            // self.bundle.remove_item(task_to_remove); // Removed by truncate above
            if !self.path.remove_item(task_to_remove) {
                panic!("Task {} in bundle but not in path", task_to_remove.id());
            }

            // Only reset bid if we are currently recorded as the winning agent.
            // NOTE: This logic is primarily for SUCCESSOR tasks.
            // For the task that triggered the release, we likely already updated the state (Action::Update/Reset).
            // However, successor tasks are removed due to path constraints (not direct messages),
            // so we must explicitly reset our winning status for them if we held it.
            if let Some(BidInfo::Winner(w, _, t)) = self.bids.get(&task_to_remove.id())
                && *w == self.agent.id()
            {
                self.bids.insert(task_to_remove.id(), BidInfo::None(*t));
            }
        }
        removed_ids
    }
}

/// Determine the action to take based on the consensus rules
fn determine_action(
    sender_k: AgentId,
    self_i: AgentId,
    sender_bid: &BidInfo,
    receiver_bid: &BidInfo,
) -> Action {
    let (y_kj, z_kj, t_kj) = match sender_bid {
        BidInfo::Winner(w, s, t) => (*s, Some(*w), *t),
        BidInfo::None(t) => (Score::default(), None, *t),
    };

    let (y_ij, z_ij, t_ij) = match receiver_bid {
        BidInfo::Winner(w, s, t) => (*s, Some(*w), *t),
        BidInfo::None(t) => (Score::default(), None, *t),
    };

    // Helper closure for score comparison with tie-breaking
    let is_better_bid = |score_new: Score,
                         winner_new: AgentId,
                         score_current: Score,
                         winner_current: AgentId|
     -> bool {
        if score_new > score_current {
            true
        } else if score_new == score_current {
            // Tie-breaking: Prefer smaller AgentId
            winner_new < winner_current
        } else {
            false
        }
    };

    if let Some(winner_k) = z_kj {
        // Sender thinks `winner_k` is the winner
        if let Some(winner_i) = z_ij {
            // Receiver thinks `winner_i` is the winner

            if winner_k == winner_i {
                // Both agree on the winner: Update if sender has newer info
                if t_kj > t_ij {
                    Action::Update
                } else {
                    Action::Leave
                }
            } else if winner_k == sender_k {
                // Sender thinks Sender wins
                if winner_i == self_i {
                    // Receiver thinks Receiver wins: Compare scores
                    if is_better_bid(y_kj, winner_k, y_ij, winner_i) {
                        Action::Update
                    } else {
                        Action::Leave
                    }
                } else {
                    // Receiver thinks `m` (other) wins
                    // Compare Sender (k) vs `m`
                    if is_better_bid(y_kj, winner_k, y_ij, winner_i) {
                        Action::Update
                    } else {
                        Action::Leave
                    }
                }
            } else if winner_k == self_i {
                // Sender thinks Receiver wins
                if winner_i == sender_k {
                    // Receiver thinks Sender wins -> Contradiction -> Reset
                    Action::Reset
                } else {
                    // Receiver thinks `m` wins -> Reset (Sender thought I won, but I think `m` wins... weird, but Reset is safe)
                    Action::Reset
                }
            } else {
                // Sender thinks `m` (other) wins
                if winner_i == self_i {
                    // Receiver thinks Receiver wins: Compare `m` vs Receiver
                    if is_better_bid(y_kj, winner_k, y_ij, winner_i) {
                        Action::Update
                    } else {
                        Action::Leave
                    }
                } else if winner_i == sender_k {
                    // Receiver thinks Sender wins: Sender conceded to `m` -> Update to `m`
                    Action::Update
                } else {
                    // Receiver thinks `n` wins: Compare `m` vs `n`
                    if is_better_bid(y_kj, winner_k, y_ij, winner_i) {
                        Action::Update
                    } else {
                        Action::Leave
                    }
                }
            }
        } else {
            // Receiver thinks None
            Action::Update
        }
    } else {
        // Sender thinks None
        if let Some(winner_i) = z_ij {
            if winner_i == sender_k {
                // Receiver thinks Sender wins, but Sender says None -> Update (to None)
                Action::Update
            } else {
                // Receiver thinks I or m wins -> Ignore Sender's None
                Action::Leave
            }
        } else {
            // Both None: Update if newer timestamp (though functionally doesn't matter much)
            if t_kj > t_ij {
                Action::Update
            } else {
                Action::Leave
            }
        }
    }
}
