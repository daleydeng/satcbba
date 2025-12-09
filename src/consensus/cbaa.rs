//! Consensus-Based Auction Algorithm (CBAA) implementation

use crate::consensus::types::{Agent, AgentId, Task, TaskId, Score, Timestamp, ConsensusMessage, BidInfo};
use crate::error::Error;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

/// Configuration for CBAA algorithm
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    /// Maximum number of tasks per agent (always 1 for CBAA)
    pub max_tasks_per_agent: usize,

    /// Maximum number of iterations to prevent infinite loops
    pub max_iterations: u64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_tasks_per_agent: 1,
            max_iterations: 100,
        }
    }
}

/// Result type for auction operations
#[derive(Debug, Clone, PartialEq)]
pub enum AuctionResult<T: Task> {
    TaskSelected(T),
    NoValidTasks,
    AlreadyAssigned,
}

/// Trait for scoring functions that evaluate task assignments
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

    /// Calculate the score/reward for an agent to perform a task
    ///
    /// # Returns
    ///
    /// A score value where higher values indicate better assignments
    /// (rewards).
    fn calc(&self, agent: &A, task: &T) -> Score;
}

/// State tracking for auction algorithms
#[derive(Debug, Clone, Default)]
pub struct AuctionState {
    pub iteration: u64,
    /// Winning bids for each task (y_i in paper)
    pub winning_bids: HashMap<TaskId, Score>,
    /// Winning agents for each task (z_i in paper)
    pub winning_agents: HashMap<TaskId, AgentId>,
    /// Timestamps for conflict resolution (Agent-level)
    pub timestamps: HashMap<AgentId, Timestamp>,
    /// Timestamps for task information (Task-level)
    pub task_timestamps: HashMap<TaskId, Timestamp>,
    pub converged: bool,
}

impl AuctionState {
    pub fn new() -> Self {
        Self::default()
    }
}

/// Consensus-Based Auction Algorithm (CBAA) implementation
///
/// CBAA is designed for single-assignment problems where each agent
/// can be assigned at most one task. It combines auction-based task
/// selection with consensus-based conflict resolution.
///
/// # Type Parameters
///
/// * `T` - Task type that implements the `Task` trait
/// * `A` - Agent ID type that implements the `AgentId` trait
/// * `C` - Scoring function type that implements `ScoreFunction<T, A>`
///
/// # Examples
///
/// ```
/// use cbbadds::*;
/// use cbbadds::cbaa::ScoreFunction;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
/// struct SimpleTask(u32);
/// impl Task for SimpleTask {
///     fn id(&self) -> TaskId { TaskId(self.0) }
/// }
///
/// #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize, PartialOrd, Ord)]
/// struct SimpleAgent(u32);
/// impl Agent for SimpleAgent {
///     fn id(&self) -> AgentId { AgentId(self.0) }
/// }
///
/// struct SimpleScoreFunction;
///
/// impl ConstraintChecker<SimpleTask, SimpleAgent> for SimpleScoreFunction {
///     fn check(&self, _agent: &SimpleAgent, _task: &SimpleTask) -> bool {
///         true
///     }
/// }
///
/// impl ScoreFunction<SimpleTask, SimpleAgent> for SimpleScoreFunction {
///     fn calc(&self, _agent: &SimpleAgent, task: &SimpleTask) -> Score {
///         Score(100 - task.0)
///     }
/// }
///
/// let mut cbaa = CBAA::new(
///     SimpleAgent(1),
///     SimpleScoreFunction,
///     vec![SimpleTask(1), SimpleTask(2)],
///     None
/// );
///
/// let result = cbaa.auction_phase();
/// ```
#[derive(Debug)]
pub struct CBAA<T: Task, A: Agent, C: ScoreFunction<T, A>> {
    pub agent: A,
    pub config: Config,
    pub state: AuctionState,
    pub score_function: C,
    pub available_tasks: Vec<T>,
    pub bundle: Option<T>,
    _phantom: PhantomData<(T, A)>,
}

impl<T: Task, A: Agent, C: ScoreFunction<T, A>> CBAA<T, A, C> {
    pub fn new(
        agent: A,
        score_function: C,
        available_tasks: Vec<T>,
        config: Option<Config>,
    ) -> Self {
        let config = config.unwrap_or_else(|| {
            let mut cfg = Config::default();
            cfg.max_tasks_per_agent = 1; // CBAA is single-assignment
            cfg
        });

        Self {
            agent,
            config,
            state: AuctionState::default(),
            score_function,
            available_tasks,
            bundle: None,
            _phantom: PhantomData,
        }
    }

    /// Run one iteration of the CBAA algorithm
    pub fn run_iteration(
        &mut self,
        incoming_messages: Vec<ConsensusMessage>,
    ) -> Result<(Vec<T>, bool), Error> {
        self.auction_phase()?;
        self.consensus_phase(incoming_messages)?;
        self.state.iteration += 1;

        let bundle_vec = self.bundle.clone().map(|t| vec![t]).unwrap_or_default();
        Ok((bundle_vec, self.state.converged || self.state.iteration >= self.config.max_iterations))
    }

    /// Execute the auction phase
    ///
    /// Selects the best available task if not already assigned.
    pub fn auction_phase(&mut self) -> Result<Option<T>, Error> {
        if self.bundle.is_some() {
            return Ok(self.bundle.clone());
        }

        let valid_tasks = self.get_valid_tasks();
        if valid_tasks.is_empty() {
            return Ok(None);
        }

        match self.select_best_task(&valid_tasks) {
            Ok((selected_task, bid)) => {
                self.state.winning_bids.insert(selected_task.id(), bid);
                self.state
                    .winning_agents
                    .insert(selected_task.id(), self.agent.id());
                let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros() as u64;
                self.state
                    .task_timestamps
                    .insert(selected_task.id(), Timestamp(now));

                // Update context for future calculations
                self.bundle = Some(selected_task.clone());
                Ok(Some(selected_task))
            }
            Err(_) => Ok(None),
        }
    }

    /// Execute the consensus phase
    ///
    /// Processes messages from other agents and resolves conflicts.
    pub fn consensus_phase(
        &mut self,
        messages: Vec<ConsensusMessage>,
    ) -> Result<(), Error> {
        let empty_messages = messages.is_empty();
        for message in messages {
            self.process_consensus_message(message)?;
        }

        // Check if we lost our assigned task
        if let Some(task) = &self.bundle {
            if let Some(winner) = self.state.winning_agents.get(&task.id()) {
                if *winner != self.agent.id() {
                    self.bundle = None;
                }
            }
        }

        // Convergence check logic would go here
        // For simple CBAA, we can check if bids/assignments stopped changing
        // This is a simplified check
        if self.state.iteration > 0 && empty_messages {
            // self.state.converged = true;
        }

        Ok(())
    }

    pub fn get_winning_bids(&self) -> HashMap<TaskId, Score> {
        self.state.winning_bids.clone()
    }

    pub fn get_winning_agents(&self) -> HashMap<TaskId, AgentId> {
        self.state.winning_agents.clone()
    }

    pub fn get_task_timestamps(&self) -> HashMap<TaskId, Timestamp> {
        self.state.task_timestamps.clone()
    }

    pub fn has_converged(&self) -> bool {
        // Use config to determine convergence criteria
        // For now, simple convergence check - could be enhanced with config parameters
        self.state.converged
            || self.state.iteration >= self.config.max_iterations
    }

    pub fn get_iteration(&self) -> u64 {
        self.state.iteration
    }

    pub fn reset(&mut self) {
        self.state = AuctionState::new();
        self.bundle = None;
    }

    /// Get the agent ID
    pub fn get_agent_id(&self) -> AgentId {
        self.agent.id()
    }

    // Private helper methods

    fn get_valid_tasks(&self) -> Vec<T> {
        self.available_tasks
            .iter()
            .filter(|task| {
                // Task is valid if:
                // 1. We satisfy constraints
                // 2. Our score is better than current winning bid
                if !self.score_function.check(&self.agent, task) {
                    return false;
                }

                let score = self.score_function.calc(
                    &self.agent,
                    task,
                );

                let current_winning_bid = self
                    .state
                    .winning_bids
                    .get(&task.id())
                    .copied()
                    .unwrap_or_default();

                score > current_winning_bid
            })
            .cloned()
            .collect()
    }

    fn select_best_task(&self, valid_tasks: &[T]) -> Result<(T, Score), Error> {
        valid_tasks
            .iter()
            .map(|task| {
                let bid = self
                    .score_function
                    .calc(&self.agent, task);
                (task.clone(), bid)
            })
            .max_by(|a, b| a.1.cmp(&b.1))
            .ok_or(Error::InvalidBid)
    }

    fn process_consensus_message(
        &mut self,
        message: ConsensusMessage,
    ) -> Result<(), Error> {
        let ConsensusMessage { agent_id: _, bids: updates } = message;

        for (task_id, bid_info) in updates {
            let (bid, winner, _timestamp) = match bid_info {
                BidInfo::Winner(w, s, t) => (s, Some(w), t),
                BidInfo::None(t) => (Score::default(), None, t),
            };

            let current_bid = self
                .state
                .winning_bids
                .get(&task_id)
                .copied()
                .unwrap_or_default();

            let current_winner = self.state.winning_agents.get(&task_id).copied();

            let mut update = false;

            if bid > current_bid {
                update = true;
            } else if bid == current_bid {
                // Tie-breaking
                // If bids are equal, prefer the one with "better" winner ID (e.g., smaller)
                // Or if we are the current winner, we defend our win unless other is better
                // Standard CBBA: if z_ij == z_ik (same winner), update timestamp.
                // If z_ij != z_ik, compare bids.

                // Simple consistent tie-breaking:
                // If bids equal, prefer smaller AgentID?
                // BUT we must respect current winner if they are resetting or updating.

                // If I think winner is X, and message says winner is Y.
                // If X == Y, no conflict on winner.
                // If X != Y, we have a conflict with equal bids.
                if let Some(curr_win) = current_winner {
                    // We use a consistent rule: e.g. smallest AgentId wins
                    // But wait, if Y claims to be winner with same score as X,
                    // who is right?
                    // Usually the one who bid first? But we don't know time.
                    // So we use AgentId.
                    // Assuming A: AgentId implements Ord (it does).
                    // If winner < curr_win, we update.
                    if winner != Some(curr_win) {
                        // If the new winner is "smaller" (higher priority), we update.
                        // Note: We trust the bid value is accurate for that agent.
                        if winner < Some(curr_win) {
                            update = true;
                        }
                    }
                } else {
                    update = true;
                }
            } else {
                // bid < current_bid
                // Only update if it comes from the current winner (reset)
                if let Some(curr_win) = current_winner {
                    // If the message says "Winner is X, Bid is Low", and we think "Winner is X, Bid is High".
                    // This is a reset from X.
                    if Some(curr_win) == winner {
                        update = true;
                    }
                }
            }

            if update {
                self.state.winning_bids.insert(task_id, bid);
                if let Some(w) = winner {
                    self.state.winning_agents.insert(task_id, w);
                } else {
                    self.state.winning_agents.remove(&task_id);
                }
                // Timestamp update not strictly needed for CBAA but good for consistency
                // self.state.task_timestamps.insert(task_id, timestamp);
            }
        }
        Ok(())
    }
}
