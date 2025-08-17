// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024 Ian Philip Eglin
//! Consensus-Based Auction Algorithm (CBAA) implementation

use crate::config::AuctionConfig;
use crate::error::AuctionError;
use crate::network::ConsensusMessage;
use crate::types::*;
use std::collections::HashMap;
use std::marker::PhantomData;

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
/// * `C` - Cost function type that implements `CostFunction<T, A>`
///
/// # Examples
///
/// ```
/// use kefli::*;
///
/// #[derive(Debug, Clone, PartialEq, Eq, Hash)]
/// struct SimpleTask(u32);
///
/// #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
/// struct SimpleAgent(u32);
///
/// struct SimpleCostFunction;
///
/// impl CostFunction<SimpleTask, SimpleAgent> for SimpleCostFunction {
///     fn calculate_cost(&self, _agent: SimpleAgent, task: &SimpleTask, _context: &AssignmentContext<SimpleTask, SimpleAgent>) -> f64 {
///         100.0 - task.0 as f64
///     }
/// }
///
/// let mut cbaa = CBAA::new(
///     SimpleAgent(1),
///     SimpleCostFunction,
///     vec![SimpleTask(1), SimpleTask(2)],
///     None
/// );
///
/// let result = cbaa.auction_phase();
/// ```
#[derive(Debug)]
pub struct CBAA<T: Task, A: AgentId, C: CostFunction<T, A>> {
    /// Unique identifier for this agent
    agent_id: A,
    /// Configuration parameters
    config: AuctionConfig,
    /// Current algorithm state
    state: AuctionState<T, A>,
    /// Cost function for evaluating task assignments
    cost_function: C,
    /// Available tasks to bid on
    available_tasks: Vec<T>,
    /// Assignment context for cost calculations
    context: AssignmentContext<T, A>,
    /// Phantom data for type safety
    _phantom: PhantomData<(T, A)>,
}

impl<T: Task, A: AgentId, C: CostFunction<T, A>> CBAA<T, A, C> {
    /// Create a new CBAA instance
    ///
    /// # Arguments
    ///
    /// * `agent_id` - Unique identifier for this agent
    /// * `cost_function` - Function to evaluate task assignments
    /// * `available_tasks` - Initial list of available tasks
    /// * `config` - Optional configuration (uses default if None)
    ///
    /// # Returns
    ///
    /// A new CBAA instance ready to participate in auctions
    pub fn new(
        agent_id: A,
        cost_function: C,
        available_tasks: Vec<T>,
        config: Option<AuctionConfig>,
    ) -> Self {
        let config = config.unwrap_or_else(|| {
            let mut cfg = AuctionConfig::default();
            cfg.max_tasks_per_agent = 1; // CBAA is single-assignment
            cfg
        });

        Self {
            agent_id,
            config,
            state: AuctionState::default(),
            cost_function,
            available_tasks,
            context: AssignmentContext::default(),
            _phantom: PhantomData,
        }
    }

    /// Phase 1: Auction Process - Select task and place bid
    ///
    /// This method implements Algorithm 1 from the paper. It evaluates
    /// available tasks and selects the one with the highest score.
    ///
    /// # Returns
    ///
    /// * `Ok(AuctionResult::TaskSelected(task))` - Successfully selected a task
    /// * `Ok(AuctionResult::AlreadyAssigned)` - Agent already has an assignment
    /// * `Ok(AuctionResult::NoValidTasks)` - No valid tasks available
    /// * `Err(AuctionError)` - An error occurred during the auction
    pub fn auction_phase(&mut self) -> Result<AuctionResult<T>, AuctionError> {
        // Check if agent is already assigned (CBAA allows only one task)
        if self.has_assignment() {
            return Ok(AuctionResult::AlreadyAssigned);
        }

        // Generate list of valid tasks (h_i in paper)
        let valid_tasks = self.get_valid_tasks();

        if valid_tasks.is_empty() {
            return Ok(AuctionResult::NoValidTasks);
        }

        // Select task with highest score
        let (selected_task, bid_value) = self.select_best_task(&valid_tasks)?;

        // Update local state
        self.state.assignments.insert(selected_task.clone(), true);
        self.state
            .winning_bids
            .insert(selected_task.clone(), bid_value);
        self.state
            .winning_agents
            .insert(selected_task.clone(), self.agent_id);

        // Update context for future calculations
        self.context.current_assignments = vec![selected_task.clone()];

        Ok(AuctionResult::TaskSelected(selected_task))
    }

    /// Phase 2: Consensus Process - Update winning bids based on neighbor information
    ///
    /// This method implements Algorithm 2 from the paper. It processes consensus
    /// messages from other agents and updates the local state accordingly.
    ///
    /// # Arguments
    ///
    /// * `messages` - Consensus messages received from other agents
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an AuctionError on failure
    pub fn consensus_phase(
        &mut self,
        messages: Vec<ConsensusMessage<A, T>>,
    ) -> Result<(), AuctionError> {
        for message in messages {
            self.process_consensus_message(message)?;
        }

        // Check if this agent was outbid and release assignment if necessary
        self.check_and_release_outbid_tasks()?;

        self.state.iteration += 1;
        Ok(())
    }

    /// Get the configuration for this CBAA instance
    pub fn get_config(&self) -> &AuctionConfig {
        &self.config
    }

    /// Check if the algorithm has converged to a stable assignment
    ///
    /// # Returns
    ///
    /// true if the algorithm has converged, false otherwise
    pub fn has_converged(&self) -> bool {
        // Use config to determine convergence criteria
        // For now, simple convergence check - could be enhanced with config parameters
        self.state.converged || self.state.iteration > (self.config.network_diameter as u64 * 10)
    }

    /// Get the current task assignment for this agent
    ///
    /// # Returns
    ///
    /// Some(task) if the agent has an assignment, None otherwise
    pub fn get_assignment(&self) -> Option<T> {
        self.state
            .assignments
            .iter()
            .find(|(_, assigned)| **assigned)
            .map(|(task, _)| task.clone())
    }

    /// Get current winning bids (for sending to neighbors)
    ///
    /// # Returns
    ///
    /// A HashMap of tasks to their current winning bid values
    pub fn get_winning_bids(&self) -> HashMap<T, f64> {
        self.state.winning_bids.clone()
    }

    /// Get current winning agents (for sending to neighbors)
    ///
    /// # Returns
    ///
    /// A HashMap of tasks to their current winning agents
    pub fn get_winning_agents(&self) -> HashMap<T, A> {
        self.state.winning_agents.clone()
    }

    /// Update the available tasks list
    ///
    /// # Arguments
    ///
    /// * `tasks` - New list of available tasks
    pub fn update_available_tasks(&mut self, tasks: Vec<T>) {
        self.available_tasks = tasks;
    }

    /// Update assignment context with external information
    ///
    /// # Arguments
    ///
    /// * `context` - New assignment context
    pub fn update_context(&mut self, context: AssignmentContext<T, A>) {
        self.context = context;
    }

    /// Get the current iteration number
    pub fn get_iteration(&self) -> u64 {
        self.state.iteration
    }

    /// Get the agent ID
    pub fn get_agent_id(&self) -> A {
        self.agent_id
    }

    // Private helper methods

    fn has_assignment(&self) -> bool {
        self.state.assignments.values().any(|&assigned| assigned)
    }

    fn get_valid_tasks(&self) -> Vec<T> {
        self.available_tasks
            .iter()
            .filter(|task| {
                let current_bid =
                    self.cost_function
                        .calculate_cost(self.agent_id, task, &self.context);
                let winning_bid = self
                    .state
                    .winning_bids
                    .get(task)
                    .copied()
                    .unwrap_or(f64::NEG_INFINITY);
                current_bid > winning_bid
            })
            .cloned()
            .collect()
    }

    fn select_best_task(&self, valid_tasks: &[T]) -> Result<(T, f64), AuctionError> {
        valid_tasks
            .iter()
            .map(|task| {
                let bid = self
                    .cost_function
                    .calculate_cost(self.agent_id, task, &self.context);
                (task.clone(), bid)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or(AuctionError::InvalidBid)
    }

    fn process_consensus_message(
        &mut self,
        message: ConsensusMessage<A, T>,
    ) -> Result<(), AuctionError> {
        match message {
            ConsensusMessage::WinningBids { agent_id, bids } => {
                for (task, bid) in bids {
                    let current_bid = self
                        .state
                        .winning_bids
                        .get(&task)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY);
                    if bid > current_bid {
                        self.state.winning_bids.insert(task.clone(), bid);
                        self.state.winning_agents.insert(task, agent_id);
                    }
                }
            }
            ConsensusMessage::WinningAgents {
                agent_id: _,
                assignments,
            } => {
                for (task, agent) in assignments {
                    self.state.winning_agents.insert(task, agent);
                }
            }
            ConsensusMessage::Timestamp {
                agent_id: _,
                timestamps,
            } => {
                for (agent, timestamp) in timestamps {
                    self.state.timestamps.insert(agent, timestamp);
                }
            }
        }
        Ok(())
    }

    fn check_and_release_outbid_tasks(&mut self) -> Result<(), AuctionError> {
        let mut tasks_to_release = Vec::new();

        for (task, _) in self
            .state
            .assignments
            .iter()
            .filter(|(_, assigned)| **assigned)
        {
            if let Some(&winning_agent) = self.state.winning_agents.get(task) {
                if winning_agent != self.agent_id {
                    tasks_to_release.push(task.clone());
                }
            }
        }

        for task in tasks_to_release {
            self.state.assignments.insert(task.clone(), false);
            self.context.current_assignments.retain(|t| t != &task);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    struct TestTask(u32);

    #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
    struct TestAgent(u32);

    struct TestCostFunction;

    impl CostFunction<TestTask, TestAgent> for TestCostFunction {
        fn calculate_cost(
            &self,
            _agent: TestAgent,
            task: &TestTask,
            _context: &AssignmentContext<TestTask, TestAgent>,
        ) -> f64 {
            100.0 - task.0 as f64
        }
    }

    #[test]
    fn test_cbaa_creation() {
        let cost_fn = TestCostFunction;
        let tasks = vec![TestTask(1), TestTask(2)];

        let cbaa = CBAA::new(TestAgent(1), cost_fn, tasks, None);
        assert!(!cbaa.has_converged());
        assert!(cbaa.get_assignment().is_none());
        assert_eq!(cbaa.get_agent_id(), TestAgent(1));
    }

    #[test]
    fn test_auction_phase() {
        let cost_fn = TestCostFunction;
        let tasks = vec![TestTask(1), TestTask(5)];

        let mut cbaa = CBAA::new(TestAgent(1), cost_fn, tasks, None);

        // Should select the task with lower ID (higher cost)
        let result = cbaa.auction_phase().unwrap();
        match result {
            AuctionResult::TaskSelected(task) => {
                assert_eq!(task, TestTask(1));
            }
            _ => panic!("Expected task selection"),
        }
    }

    #[test]
    fn test_already_assigned() {
        let cost_fn = TestCostFunction;
        let tasks = vec![TestTask(1)];

        let mut cbaa = CBAA::new(TestAgent(1), cost_fn, tasks, None);

        // First auction should succeed
        let result1 = cbaa.auction_phase().unwrap();
        assert!(matches!(result1, AuctionResult::TaskSelected(_)));

        // Second auction should return AlreadyAssigned
        let result2 = cbaa.auction_phase().unwrap();
        assert!(matches!(result2, AuctionResult::AlreadyAssigned));
    }
}
