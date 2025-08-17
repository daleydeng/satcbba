// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024 Ian Philip Eglin
//! Simple example demonstrating basic CBAA usage

use kefli::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct DeliveryTask {
    destination: String,
    urgency: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
struct DeliveryAgent {
    id: u32,
}

struct DeliveryCostFunction {
    agent_location: String,
}

impl CostFunction<DeliveryTask, DeliveryAgent> for DeliveryCostFunction {
    fn calculate_cost(
        &self,
        _agent: DeliveryAgent,
        task: &DeliveryTask,
        _context: &AssignmentContext<DeliveryTask, DeliveryAgent>,
    ) -> f64 {
        // Simple cost based on destination match and urgency
        let location_bonus = if self.agent_location == task.destination {
            50.0
        } else {
            0.0
        };
        let urgency_bonus = task.urgency as f64 * 10.0;

        location_bonus + urgency_bonus
    }
}

fn main() -> Result<(), AuctionError> {
    println!("Simple CBAA Auction Example");
    println!("===========================");

    // Create tasks
    let tasks = vec![
        DeliveryTask {
            destination: "Downtown".to_string(),
            urgency: 3,
        },
        DeliveryTask {
            destination: "Airport".to_string(),
            urgency: 5,
        },
        DeliveryTask {
            destination: "Suburbs".to_string(),
            urgency: 2,
        },
    ];

    // Create three delivery agents with different locations
    let agents = vec![
        (DeliveryAgent { id: 1 }, "Downtown"),
        (DeliveryAgent { id: 2 }, "Airport"),
        (DeliveryAgent { id: 3 }, "Suburbs"),
    ];

    let mut cbaas = Vec::new();

    // Initialize CBAA for each agent
    for (agent, location) in agents {
        let cost_function = DeliveryCostFunction {
            agent_location: location.to_string(),
        };

        let cbaa = CBAA::new(
            agent,
            cost_function,
            tasks.clone(),
            Some(AuctionConfig::cbaa()),
        );

        cbaas.push((agent, cbaa));
    }

    println!("Running auction phase for all agents...");

    // Phase 1: All agents bid
    let mut auction_results = Vec::new();
    for (agent, cbaa) in &mut cbaas {
        match cbaa.auction_phase()? {
            AuctionResult::TaskSelected(task) => {
                println!("Agent {:?} bid on task: {:?}", agent, task);
                auction_results.push((*agent, Some(task)));
            }
            AuctionResult::NoValidTasks => {
                println!("Agent {:?} found no valid tasks", agent);
                auction_results.push((*agent, None));
            }
            AuctionResult::AlreadyAssigned => {
                println!("Agent {:?} already assigned", agent);
                auction_results.push((*agent, None));
            }
        }
    }

    println!("\nExchanging winning bids...");

    // Phase 2: Exchange winning bids (simplified - in real system this would be over network)
    let mut all_messages = Vec::new();
    for (agent, cbaa) in &cbaas {
        let winning_bids = cbaa.get_winning_bids();
        if !winning_bids.is_empty() {
            let message = ConsensusMessage::WinningBids {
                agent_id: *agent,
                bids: winning_bids,
            };
            all_messages.push(message);
        }
    }

    // Apply consensus
    for (agent, cbaa) in &mut cbaas {
        // Each agent receives messages from all other agents
        let other_messages: Vec<_> = all_messages
            .iter()
            .filter(|msg| match msg {
                ConsensusMessage::WinningBids { agent_id, .. } => *agent_id != *agent,
                _ => true,
            })
            .cloned()
            .collect();

        cbaa.consensus_phase(other_messages)?;
    }

    println!("\nFinal assignments after consensus:");

    // Check final assignments
    for (agent, cbaa) in &cbaas {
        match cbaa.get_assignment() {
            Some(task) => {
                println!("Agent {:?} assigned to: {:?}", agent, task);
            }
            None => {
                println!("Agent {:?} has no assignment", agent);
            }
        }
    }

    Ok(())
}
