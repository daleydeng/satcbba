// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2025 Ian Philip Eglin
//! Integration tests for the kefli library

use kefli::*;
use std::collections::HashMap;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct SimpleTask {
    id: u32,
    priority: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
struct SimpleAgent {
    id: u32,
}

struct SimpleNetworkHandler {
    agent_id: SimpleAgent,
    neighbors: Vec<SimpleAgent>,
    sent_messages: Vec<ConsensusMessage<SimpleAgent, SimpleTask>>,
    received_messages: Vec<ConsensusMessage<SimpleAgent, SimpleTask>>,
}

impl SimpleNetworkHandler {
    fn new(agent_id: SimpleAgent, neighbors: Vec<SimpleAgent>) -> Self {
        Self {
            agent_id,
            neighbors,
            sent_messages: Vec::new(),
            received_messages: Vec::new(),
        }
    }

    fn add_received_message(&mut self, message: ConsensusMessage<SimpleAgent, SimpleTask>) {
        self.received_messages.push(message);
    }
}

impl NetworkHandler<SimpleAgent, SimpleTask> for SimpleNetworkHandler {
    fn send_message(&mut self, message: ConsensusMessage<SimpleAgent, SimpleTask>) -> Result<(), AuctionError> {
        self.sent_messages.push(message);
        Ok(())
    }

    fn receive_messages(&mut self) -> Result<Vec<ConsensusMessage<SimpleAgent, SimpleTask>>, AuctionError> {
        let messages = self.received_messages.clone();
        self.received_messages.clear();
        Ok(messages)
    }

    fn get_neighbors(&self) -> Vec<SimpleAgent> {
        self.neighbors.clone()
    }
}

struct SimpleCostFunction;

impl CostFunction<SimpleTask, SimpleAgent> for SimpleCostFunction {
    fn calculate_cost(&self, agent: SimpleAgent, task: &SimpleTask, _context: &AssignmentContext<SimpleTask, SimpleAgent>) -> f64 {
        // Simple cost: agent preference + task priority
        let agent_preference = match agent.id % 3 {
            0 => 10.0,
            1 => 5.0,
            _ => 1.0,
        };
        agent_preference + task.priority as f64
    }
}

#[test]
fn test_multi_agent_auction() {
    let task1 = SimpleTask { id: 1, priority: 5 };
    let task2 = SimpleTask { id: 2, priority: 3 };
    let tasks = vec![task1.clone(), task2.clone()];

    // Create two agents
    let agent1 = SimpleAgent { id: 1 };
    let agent2 = SimpleAgent { id: 2 };

    let mut cbaa1 = CBAA::new(agent1, SimpleCostFunction, tasks.clone(), None);
    let mut cbaa2 = CBAA::new(agent2, SimpleCostFunction, tasks, None);

    // Phase 1: Both agents bid
    let result1 = cbaa1.auction_phase().unwrap();
    let result2 = cbaa2.auction_phase().unwrap();

    // Both should get a task
    assert!(matches!(result1, AuctionResult::TaskSelected(_)));
    assert!(matches!(result2, AuctionResult::TaskSelected(_)));

    // Exchange winning bids
    let bids1 = cbaa1.get_winning_bids();
    let bids2 = cbaa2.get_winning_bids();

    let message1 = ConsensusMessage::WinningBids {
        agent_id: agent1,
        bids: bids1,
    };
    let message2 = ConsensusMessage::WinningBids {
        agent_id: agent2,
        bids: bids2,
    };

    // Phase 2: Consensus
    cbaa1.consensus_phase(vec![message2]).unwrap();
    cbaa2.consensus_phase(vec![message1]).unwrap();

    // Check final assignments
    let assignment1 = cbaa1.get_assignment();
    let assignment2 = cbaa2.get_assignment();

    // Ensure no conflicts (different tasks or one released)
    if let (Some(task_a), Some(task_b)) = (assignment1, assignment2) {
        assert_ne!(task_a, task_b, "Agents should not have the same task");
    }
}

#[test]
fn test_cbba_multi_assignment() {
    let tasks = vec![
        SimpleTask { id: 1, priority: 5 },
        SimpleTask { id: 2, priority: 3 },
#[test]
fn test_cbba_multi_assignment() {
    let tasks = vec![
        SimpleTask { id: 1, priority: 5 },
        SimpleTask { id: 2, priority: 3 },
        SimpleTask { id: 3, priority: 4 },
        SimpleTask { id: 4, priority: 2 },
    ];

    let agent = SimpleAgent { id: 1 };
    let config = AuctionConfig::cbba(3); // Allow up to 3 tasks

    let mut cbba = CBBA::new(agent, SimpleCostFunction, tasks, Some(config));

    // Run bundle construction
    let added_tasks = cbba.bundle_construction_phase().unwrap();

    // Should add multiple tasks
    assert!(!added_tasks.is_empty());
    assert!(added_tasks.len() <= 3);

    // Bundle should contain the added tasks
    for task in &added_tasks {
        assert!(cbba.get_bundle().contains(task));
    }

    // Path should have the same number of tasks
    assert_eq!(cbba.get_path().len(), added_tasks.len());
}

#[test] 
fn test_config_builder() {
    let config = AuctionConfig::cbba(5)
        .with_network_diameter(10)
        .with_timeout_ms(2000)
        .with_async_resolution(false);

    assert_eq!(config.max_tasks_per_agent, 5);
    assert_eq!(config.network_diameter, 10);
    assert_eq!(config.consensus_timeout, Duration::from_millis(2000));
    assert!(!config.async_conflict_resolution);
}

#[test]
fn test_error_handling() {
    let agent = SimpleAgent { id: 1 };
    let mut cbaa = CBAA::new(agent, SimpleCostFunction, vec![], None);

    // Should return NoValidTasks when no tasks available
    let result = cbaa.auction_phase().unwrap();
    assert!(matches!(result, AuctionResult::NoValidTasks));
}
