//! Core types and traits for the auction algorithms

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;

/// Core trait that task types must implement
///
/// This trait is automatically implemented for any type that satisfies
/// the bounds: Clone + Debug + PartialEq + Eq + Hash
pub trait Task: Clone + Debug + PartialEq + Eq + Hash {}

/// Core trait for agent identifiers
///
/// This trait is automatically implemented for any type that satisfies
/// the bounds: Clone + Debug + PartialEq + Eq + Hash + Copy
pub trait AgentId: Clone + Debug + PartialEq + Eq + Hash + Copy {}

// Blanket implementations for common types
impl<T> Task for T where T: Clone + Debug + PartialEq + Eq + Hash {}
impl<T> AgentId for T where T: Clone + Debug + PartialEq + Eq + Hash + Copy {}

/// Result type for auction operations
#[derive(Debug, Clone, PartialEq)]
pub enum AuctionResult<T: Task> {
    /// Successfully selected a task
    TaskSelected(T),
    /// No valid tasks available for selection
    NoValidTasks,
    /// Agent already has assignments and cannot bid on more
    AlreadyAssigned,
}

/// Context information for cost calculations
///
/// This struct provides context about the current state of assignments
/// which can be used by cost functions to make informed decisions.
#[derive(Debug, Clone)]
pub struct AssignmentContext<T: Task, A: AgentId> {
    /// Currently assigned tasks for this agent
    pub current_assignments: Vec<T>,
    /// All known task assignments across agents
    pub global_assignments: HashMap<A, Vec<T>>,
    /// Additional context data that can be used by cost functions
    pub metadata: HashMap<String, String>,
}

impl<T: Task, A: AgentId> Default for AssignmentContext<T, A> {
    fn default() -> Self {
        Self {
            current_assignments: Vec::new(),
            global_assignments: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}

/// Trait for cost functions that evaluate task assignments
///
/// Cost functions determine how valuable or costly it would be for an agent
/// to take on a particular task. Higher values indicate better assignments.
pub trait CostFunction<T: Task, A: AgentId> {
    /// Calculate the cost/reward for an agent to perform a task
    ///
    /// # Arguments
    ///
    /// * `agent` - The agent that would perform the task
    /// * `task` - The task to evaluate
    /// * `context` - Current assignment context for informed decision making
    ///
    /// # Returns
    ///
    /// A floating-point value where higher values indicate better assignments
    /// (rewards). Negative values indicate costs or penalties.
    fn calculate_cost(&self, agent: A, task: &T, context: &AssignmentContext<T, A>) -> f64;
}

/// State tracking for auction algorithms
///
/// This struct maintains the current state of the auction process,
/// including bids, assignments, and convergence status.
#[derive(Debug, Clone)]
pub struct AuctionState<T: Task, A: AgentId> {
    /// Current iteration number
    pub iteration: u64,
    /// Winning bids for each task (y_i in paper)
    pub winning_bids: HashMap<T, f64>,
    /// Winning agents for each task (z_i in paper)  
    pub winning_agents: HashMap<T, A>,
    /// Agent assignments (x_i in paper)
    pub assignments: HashMap<T, bool>,
    /// Timestamps for conflict resolution
    pub timestamps: HashMap<A, u64>,
    /// Whether the algorithm has converged
    pub converged: bool,
}

impl<T: Task, A: AgentId> Default for AuctionState<T, A> {
    fn default() -> Self {
        Self {
            iteration: 0,
            winning_bids: HashMap::new(),
            winning_agents: HashMap::new(),
            assignments: HashMap::new(),
            timestamps: HashMap::new(),
            converged: false,
        }
    }
}
