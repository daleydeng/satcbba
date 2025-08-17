//! Consensus-Based Bundle Algorithm (CBBA) implementation

use crate::config::AuctionConfig;
use crate::error::AuctionError;
use crate::network::ConsensusMessage;
use crate::types::*;
use std::collections::HashMap;
use std::marker::PhantomData;

/// Bundle for CBBA - ordered list of tasks
///
/// A bundle represents a collection of tasks that an agent is bidding on
/// or has been assigned. Tasks are ordered based on when they were added.
#[derive(Debug, Clone, PartialEq)]
pub struct Bundle<T: Task> {
    tasks: Vec<T>,
    max_size: usize,
}

impl<T: Task> Bundle<T> {
    /// Create a new bundle with the specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            tasks: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Add a task to the bundle
    ///
    /// # Returns
    ///
    /// true if the task was added, false if the bundle is full or task already exists
    pub fn add_task(&mut self, task: T) -> bool {
        if self.tasks.len() < self.max_size && !self.tasks.contains(&task) {
            self.tasks.push(task);
            true
        } else {
            false
        }
    }

    /// Remove a task from the bundle
    ///
    /// # Returns
    ///
    /// true if the task was removed, false if it wasn't in the bundle
    pub fn remove_task(&mut self, task: &T) -> bool {
        if let Some(pos) = self.tasks.iter().position(|t| t == task) {
            self.tasks.remove(pos);
            true
        } else {
            false
        }
    }

    /// Remove a task and all tasks added after it
    pub fn remove_tasks_after(&mut self, task: &T) {
        if let Some(pos) = self.tasks.iter().position(|t| t == task) {
            self.tasks.truncate(pos);
        }
    }

    /// Get the number of tasks in the bundle
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Check if the bundle is empty
    pub fn is_empty(&self) -> bool {
        self.tasks.is_empty()
    }

    /// Check if the bundle is full
    pub fn is_full(&self) -> bool {
        self.tasks.len() >= self.max_size
    }

    /// Check if the bundle contains a specific task
    pub fn contains(&self, task: &T) -> bool {
        self.tasks.contains(task)
    }

    /// Get a reference to the tasks in the bundle
    pub fn tasks(&self) -> &[T] {
        &self.tasks
    }

    /// Clear all tasks from the bundle
    pub fn clear(&mut self) {
        self.tasks.clear();
    }
}

/// Path representation for CBBA - tasks ordered by execution sequence
///
/// A path represents the optimal order in which an agent should execute
/// its assigned tasks to maximize reward.
#[derive(Debug, Clone, PartialEq)]
pub struct Path<T: Task> {
    tasks: Vec<T>,
    max_size: usize,
}

impl<T: Task> Path<T> {
    /// Create a new path with the specified maximum size
    pub fn new(max_size: usize) -> Self {
        Self {
            tasks: Vec::with_capacity(max_size),
            max_size,
        }
    }

    /// Insert a task at the position that maximizes the total score
    ///
    /// # Returns
    ///
    /// Some(position) if the task was inserted, None if the path is full
    pub fn insert_task_at_best_position<A: AgentId, C: CostFunction<T, A>>(
        &mut self,
        task: T,
        cost_fn: &C,
        agent_id: A,
        context: &AssignmentContext<T, A>,
    ) -> Option<usize> {
        if self.tasks.len() >= self.max_size {
            return None;
        }

        let mut best_position = 0;
        let mut best_score = f64::NEG_INFINITY;

        // Try inserting at each possible position
        for pos in 0..=self.tasks.len() {
            let mut temp_path = self.tasks.clone();
            temp_path.insert(pos, task.clone());

            // Calculate total score for this configuration
            let mut total_score = 0.0;
            for t in &temp_path {
                total_score += cost_fn.calculate_cost(agent_id, t, context);
            }

            if total_score > best_score {
                best_score = total_score;
                best_position = pos;
            }
        }

        self.tasks.insert(best_position, task);
        Some(best_position)
    }

    /// Remove a task from the path
    ///
    /// # Returns
    ///
    /// true if the task was removed, false if it wasn't in the path
    pub fn remove_task(&mut self, task: &T) -> bool {
        if let Some(pos) = self.tasks.iter().position(|t| t == task) {
            self.tasks.remove(pos);
            true
        } else {
            false
        }
    }

    /// Get a reference to the tasks in the path
    pub fn tasks(&self) -> &[T] {
        &self.tasks
    }

    /// Get the number of tasks in the path
    pub fn len(&self) -> usize {
        self.tasks.len()
    }

    /// Clear all tasks from the path
    pub fn clear(&mut self) {
        self.tasks.clear();
    }
}

/// Consensus-Based Bundle Algorithm (CBBA) implementation
///
/// CBBA extends CBAA to handle multi-assignment problems where each agent
/// can be assigned multiple tasks in sequence. It uses bundle construction
/// to build a sequence of tasks and more sophisticated conflict resolution.
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
/// struct MultiTask(u32);
///
/// #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
/// struct MultiAgent(u32);
///
/// struct MultiCostFunction;
///
/// impl CostFunction<MultiTask, MultiAgent> for MultiCostFunction {
///     fn calculate_cost(&self, _agent: MultiAgent, task: &MultiTask, _context: &AssignmentContext<MultiTask, MultiAgent>) -> f64 {
///         task.0 as f64
///     }
/// }
///
/// let config = AuctionConfig::cbba(3); // Allow up to 3 tasks per agent
/// let mut cbba = CBBA::new(
///     MultiAgent(1),
///     MultiCostFunction,
///     vec![MultiTask(1), MultiTask(2), MultiTask(3)],
///     Some(config)
/// );
///
/// let added_tasks = cbba.bundle_construction_phase();
/// ```
#[derive(Debug)]
pub struct CBBA<T: Task, A: AgentId, C: CostFunction<T, A>> {
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
    /// Current bundle of tasks (b_i in paper)
    bundle: Bundle<T>,
    /// Current path of tasks (p_i in paper)  
    path: Path<T>,
    /// Phantom data for type safety
    _phantom: PhantomData<(T, A)>,
}

impl<T: Task, A: AgentId, C: CostFunction<T, A>> CBBA<T, A, C> {
    /// Create a new CBBA instance
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
    /// A new CBBA instance ready to participate in bundle auctions
    pub fn new(
        agent_id: A,
        cost_function: C,
        available_tasks: Vec<T>,
        config: Option<AuctionConfig>,
    ) -> Self {
        let config = config.unwrap_or_default();
        let max_tasks = config.max_tasks_per_agent;

        Self {
            agent_id,
            config,
            state: AuctionState::default(),
            cost_function,
            available_tasks,
            context: AssignmentContext::default(),
            bundle: Bundle::new(max_tasks),
            path: Path::new(max_tasks),
            _phantom: PhantomData,
        }
    }

    /// Phase 1: Bundle Construction - Build bundle by iteratively adding tasks
    ///
    /// This method implements Algorithm 3 from the paper. It continuously
    /// adds tasks to the bundle until no more valid tasks can be added.
    ///
    /// # Returns
    ///
    /// * `Ok(Vec<T>)` - List of tasks added to the bundle in this phase
    /// * `Err(AuctionError)` - An error occurred during bundle construction
    pub fn bundle_construction_phase(&mut self) -> Result<Vec<T>, AuctionError> {
        let mut added_tasks = Vec::new();

        // Continue adding tasks until bundle is full or no valid tasks remain
        while !self.bundle.is_full() {
            let valid_tasks = self.get_valid_tasks();

            if valid_tasks.is_empty() {
                break;
            }

            // Select task with highest marginal score improvement
            let (selected_task, marginal_score) = self.select_best_marginal_task(&valid_tasks)?;

            // Add to bundle and update path
            if self.bundle.add_task(selected_task.clone()) {
                // Insert task at best position in path
                self.path.insert_task_at_best_position(
                    selected_task.clone(),
                    &self.cost_function,
                    self.agent_id,
                    &self.context,
                );

                // Update winning bids and agents
                self.state
                    .winning_bids
                    .insert(selected_task.clone(), marginal_score);
                self.state
                    .winning_agents
                    .insert(selected_task.clone(), self.agent_id);

                added_tasks.push(selected_task);
            } else {
                break;
            }
        }

        // Update context with new assignments
        self.context.current_assignments = self.bundle.tasks().to_vec();

        Ok(added_tasks)
    }

    /// Phase 2: Conflict Resolution - Update based on consensus messages
    ///
    /// This method implements the conflict resolution rules from Table I in the paper.
    /// It processes consensus messages and releases tasks that have been outbid.
    ///
    /// # Arguments
    ///
    /// * `messages` - Consensus messages received from other agents
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an AuctionError on failure
    pub fn conflict_resolution_phase(
        &mut self,
        messages: Vec<ConsensusMessage<A, T>>,
    ) -> Result<(), AuctionError> {
        let mut tasks_to_release = Vec::new();

        for message in messages {
            let release_tasks = self.process_consensus_message(message)?;
            tasks_to_release.extend(release_tasks);
        }

        // Release outbid tasks and all tasks added after them
        for task in tasks_to_release {
            self.release_task_and_successors(&task);
        }

        self.state.iteration += 1;
        Ok(())
    }

    /// Get current bundle of tasks
    ///
    /// # Returns
    ///
    /// A reference to the current bundle
    pub fn get_bundle(&self) -> &Bundle<T> {
        &self.bundle
    }

    /// Get current path of tasks
    ///
    /// # Returns
    ///
    /// A reference to the current path
    pub fn get_path(&self) -> &Path<T> {
        &self.path
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

    /// Get the configuration for this CBBA instance
    pub fn get_config(&self) -> &AuctionConfig {
        &self.config
    }

    /// Check if the algorithm has converged
    ///
    /// # Returns
    ///
    /// true if the algorithm has converged, false otherwise
    pub fn has_converged(&self) -> bool {
        // Use config to determine convergence criteria
        // For now, simple convergence check - could be enhanced with config parameters
        self.state.converged
            || self.state.iteration
                > (self.config.network_diameter as u64 * self.config.max_tasks_per_agent as u64 * 5)
    }

    /// Update available tasks
    ///
    /// # Arguments
    ///
    /// * `tasks` - New list of available tasks
    pub fn update_available_tasks(&mut self, tasks: Vec<T>) {
        self.available_tasks = tasks;
    }

    /// Update assignment context
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

    fn get_valid_tasks(&self) -> Vec<T> {
        self.available_tasks
            .iter()
            .filter(|task| {
                // Task is valid if not already in bundle and our bid would be competitive
                !self.bundle.contains(task) && {
                    let marginal_score = self.calculate_marginal_score(task);
                    let current_winning_bid = self
                        .state
                        .winning_bids
                        .get(task)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY);
                    marginal_score > current_winning_bid
                }
            })
            .cloned()
            .collect()
    }

    fn select_best_marginal_task(&self, valid_tasks: &[T]) -> Result<(T, f64), AuctionError> {
        valid_tasks
            .iter()
            .map(|task| {
                let marginal_score = self.calculate_marginal_score(task);
                (task.clone(), marginal_score)
            })
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or(AuctionError::InvalidBid)
    }

    fn calculate_marginal_score(&self, task: &T) -> f64 {
        if self.bundle.contains(task) {
            return 0.0; // Task already in bundle provides no additional benefit
        }

        // Calculate score improvement from adding this task
        let current_total_score = self.calculate_path_score();

        // Create temporary path with new task inserted
        let mut temp_path = self.path.clone();
        if temp_path
            .insert_task_at_best_position(
                task.clone(),
                &self.cost_function,
                self.agent_id,
                &self.context,
            )
            .is_some()
        {
            let new_total_score = self.calculate_total_score(temp_path.tasks());
            new_total_score - current_total_score
        } else {
            0.0
        }
    }

    fn calculate_path_score(&self) -> f64 {
        self.calculate_total_score(self.path.tasks())
    }

    fn calculate_total_score(&self, tasks: &[T]) -> f64 {
        tasks
            .iter()
            .map(|task| {
                self.cost_function
                    .calculate_cost(self.agent_id, task, &self.context)
            })
            .sum()
    }

    fn process_consensus_message(
        &mut self,
        message: ConsensusMessage<A, T>,
    ) -> Result<Vec<T>, AuctionError> {
        let mut tasks_to_release = Vec::new();

        match message {
            ConsensusMessage::WinningBids { agent_id, bids } => {
                for (task, bid) in bids {
                    let current_bid = self
                        .state
                        .winning_bids
                        .get(&task)
                        .copied()
                        .unwrap_or(f64::NEG_INFINITY);

                    // Apply consensus rules from Table I in the paper
                    if bid > current_bid {
                        self.state.winning_bids.insert(task.clone(), bid);
                        self.state.winning_agents.insert(task.clone(), agent_id);

                        // Check if we need to release this task
                        if self.bundle.contains(&task) && agent_id != self.agent_id {
                            tasks_to_release.push(task);
                        }
                    }
                }
            }
            ConsensusMessage::WinningAgents {
                agent_id: _,
                assignments,
            } => {
                for (task, agent) in assignments {
                    self.state.winning_agents.insert(task.clone(), agent);

                    // Check if we lost this task
                    if self.bundle.contains(&task) && agent != self.agent_id {
                        tasks_to_release.push(task);
                    }
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

        Ok(tasks_to_release)
    }

    fn release_task_and_successors(&mut self, task: &T) {
        // Find position of task in bundle
        if let Some(pos) = self.bundle.tasks().iter().position(|t| t == task) {
            // Remove this task and all tasks added after it (as per CBBA rules)
            let tasks_to_remove: Vec<T> = self.bundle.tasks()[pos..].to_vec();

            for task_to_remove in &tasks_to_remove {
                self.bundle.remove_task(task_to_remove);
                self.path.remove_task(task_to_remove);
                self.state.winning_bids.insert(task_to_remove.clone(), 0.0);
                self.state.winning_agents.remove(task_to_remove);
            }

            // Update context
            self.context.current_assignments = self.bundle.tasks().to_vec();
        }
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
            context: &AssignmentContext<TestTask, TestAgent>,
        ) -> f64 {
            // Higher task number = higher score, but penalty for having many tasks
            let base_score = task.0 as f64;
            let workload_penalty = context.current_assignments.len() as f64 * 2.0;
            base_score - workload_penalty
        }
    }

    #[test]
    fn test_bundle_creation() {
        let mut bundle = Bundle::new(3);
        let task1 = TestTask(1);
        let task2 = TestTask(2);

        assert!(bundle.add_task(task1.clone()));
        assert!(bundle.add_task(task2));
        assert!(!bundle.add_task(task1)); // Duplicate
        assert_eq!(bundle.len(), 2);
        assert!(!bundle.is_full());
    }

    #[test]
    fn test_path_creation() {
        let path = Path::<TestTask>::new(3);
        assert_eq!(path.len(), 0);
        assert!(path.tasks().is_empty());
    }

    #[test]
    fn test_cbba_creation() {
        let cost_fn = TestCostFunction;
        let tasks = vec![TestTask(1), TestTask(2)];

        let config = AuctionConfig::cbba(3);
        let cbba = CBBA::new(TestAgent(1), cost_fn, tasks, Some(config));

        assert!(cbba.get_bundle().is_empty());
        assert_eq!(cbba.get_path().len(), 0);
        assert_eq!(cbba.get_agent_id(), TestAgent(1));
    }

    #[test]
    fn test_bundle_construction() {
        let cost_fn = TestCostFunction;
        let tasks = vec![TestTask(5), TestTask(3), TestTask(8)];

        let config = AuctionConfig::cbba(2);
        let mut cbba = CBBA::new(TestAgent(1), cost_fn, tasks, Some(config));

        let added_tasks = cbba.bundle_construction_phase().unwrap();
        assert!(!added_tasks.is_empty());
        assert!(cbba.get_bundle().len() <= 2);

        // Should prefer higher-numbered tasks initially
        if let Some(first_task) = added_tasks.first() {
            assert!(first_task.0 >= 3); // Should be a high-value task
        }
    }
}
