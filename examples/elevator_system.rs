//! Example implementation showing how to use the CBAA library for elevator dispatch
//!
//! This example demonstrates a P2P elevator system where multiple elevator controllers
//! use CBAA to bid on hall calls based on their local situational awareness.

use std::sync::mpsc::{Receiver, Sender};

// Import all the library types
use kefli::*;

/// Represents a hall call in an elevator system
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct HallCall {
    pub floor: u32,
    pub direction: Direction,
    pub timestamp: u64,
    pub priority: Priority,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Direction {
    Up,
    Down,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Priority {
    Normal,
    Emergency,
    FireService,
}

/// Unique identifier for elevator controllers
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub struct ElevatorId {
    pub controller_id: u32,
    pub elevator_number: u32,
}

/// Elevator state information for cost calculation
#[derive(Debug, Clone)]
pub struct ElevatorState {
    pub current_floor: u32,
    pub direction: Direction,
    pub speed: f64, // floors per second
    pub capacity: u32,
    pub current_load: u32,
    pub maintenance_mode: bool,
    pub energy_efficiency: f64, // 0.0 to 1.0
}

/// Cost function implementing elevator dispatch logic
pub struct ElevatorCostFunction {
    pub elevator_state: ElevatorState,
    pub building_config: BuildingConfig,
}

#[derive(Debug, Clone)]
pub struct BuildingConfig {
    pub total_floors: u32,
    pub floor_height: f64,           // meters
    pub peak_hours: Vec<(u32, u32)>, // (start_hour, end_hour) pairs
}

impl CostFunction<HallCall, ElevatorId> for ElevatorCostFunction {
    fn calculate_cost(
        &self,
        _agent: ElevatorId,
        task: &HallCall,
        context: &AssignmentContext<HallCall, ElevatorId>,
    ) -> f64 {
        // Return negative infinity if elevator is in maintenance mode
        if self.elevator_state.maintenance_mode {
            return f64::NEG_INFINITY;
        }

        let mut cost = 0.0;

        // 1. Distance cost - closer floors get higher scores
        let distance = (self.elevator_state.current_floor as i32 - task.floor as i32).abs() as f64;
        let distance_cost = -distance * 10.0; // Negative because closer is better
        cost += distance_cost;

        // 2. Direction alignment bonus
        let direction_bonus = if self.should_direction_align(task) {
            20.0
        } else {
            0.0
        };
        cost += direction_bonus;

        // 3. Load factor penalty
        let load_factor =
            self.elevator_state.current_load as f64 / self.elevator_state.capacity as f64;
        let load_penalty = -load_factor * 15.0;
        cost += load_penalty;

        // 4. Priority adjustment
        let priority_bonus = match task.priority {
            Priority::FireService => 100.0,
            Priority::Emergency => 50.0,
            Priority::Normal => 0.0,
        };
        cost += priority_bonus;

        // 5. Energy efficiency bonus
        let efficiency_bonus = self.elevator_state.energy_efficiency * 5.0;
        cost += efficiency_bonus;

        // 6. Workload penalty - prefer distributing load
        let current_assignments = context.current_assignments.len() as f64;
        let workload_penalty = -current_assignments * 8.0;
        cost += workload_penalty;

        // 7. Time-based adjustments
        cost += self.calculate_time_based_adjustment(task);

        cost
    }
}

impl ElevatorCostFunction {
    fn should_direction_align(&self, call: &HallCall) -> bool {
        match (&self.elevator_state.direction, &call.direction) {
            (Direction::Up, Direction::Up) => self.elevator_state.current_floor <= call.floor,
            (Direction::Down, Direction::Down) => self.elevator_state.current_floor >= call.floor,
            _ => false,
        }
    }

    fn calculate_time_based_adjustment(&self, _call: &HallCall) -> f64 {
        // Could implement peak hour adjustments, age of call, etc.
        0.0
    }
}

/// Network handler for P2P elevator communication
pub struct ElevatorNetworkHandler {
    pub elevator_id: ElevatorId,
    pub message_sender: Sender<(ElevatorId, ConsensusMessage<ElevatorId, HallCall>)>,
    pub message_receiver: Receiver<ConsensusMessage<ElevatorId, HallCall>>,
    pub neighbors: Vec<ElevatorId>,
}

impl NetworkHandler<ElevatorId, HallCall> for ElevatorNetworkHandler {
    fn send_message(
        &mut self,
        message: ConsensusMessage<ElevatorId, HallCall>,
    ) -> Result<(), AuctionError> {
        // In a real implementation, this would send over the network
        // For this example, we'll send to all neighbors via channels
        for neighbor in &self.neighbors {
            self.message_sender
                .send((*neighbor, message.clone()))
                .map_err(|e| AuctionError::NetworkError(format!("Send failed: {}", e)))?;
        }
        Ok(())
    }

    fn receive_messages(
        &mut self,
    ) -> Result<Vec<ConsensusMessage<ElevatorId, HallCall>>, AuctionError> {
        let mut messages = Vec::new();

        // Non-blocking receive of all pending messages
        while let Ok(message) = self.message_receiver.try_recv() {
            messages.push(message);
        }

        Ok(messages)
    }

    fn get_neighbors(&self) -> Vec<ElevatorId> {
        self.neighbors.clone()
    }
}

/// Main elevator controller using CBAA
pub struct ElevatorController {
    pub elevator_id: ElevatorId,
    pub cbaa: CBAA<HallCall, ElevatorId, ElevatorCostFunction>,
    pub network_handler: ElevatorNetworkHandler,
    pub pending_calls: Vec<HallCall>,
}

impl ElevatorController {
    pub fn new(
        elevator_id: ElevatorId,
        elevator_state: ElevatorState,
        building_config: BuildingConfig,
        network_handler: ElevatorNetworkHandler,
    ) -> Self {
        let cost_function = ElevatorCostFunction {
            elevator_state,
            building_config,
        };

        let config = AuctionConfig::cbaa()
            .with_network_diameter(4) // Typical for elevator networks
            .with_timeout_secs(1) // 1 second for real-time response
            .with_async_resolution(true);

        let cbaa = CBAA::new(
            elevator_id,
            cost_function,
            Vec::new(), // Will be updated with hall calls
            Some(config),
        );

        Self {
            elevator_id,
            cbaa,
            network_handler,
            pending_calls: Vec::new(),
        }
    }

    /// Main dispatch loop - call this periodically
    pub fn dispatch_step(&mut self) -> Result<Option<HallCall>, AuctionError> {
        // Update available tasks
        self.cbaa.update_available_tasks(self.pending_calls.clone());

        // Phase 1: Auction - try to bid on a call
        let auction_result = self.cbaa.auction_phase()?;

        // Phase 2: Consensus - process network messages
        let messages = self.network_handler.receive_messages()?;
        self.cbaa.consensus_phase(messages)?;

        // Send our current bids to neighbors
        let winning_bids = self.cbaa.get_winning_bids();
        if !winning_bids.is_empty() {
            let message = ConsensusMessage::WinningBids {
                agent_id: self.elevator_id,
                bids: winning_bids,
            };
            self.network_handler.send_message(message)?;
        }

        // Return assigned call if any
        match auction_result {
            AuctionResult::TaskSelected(call) => {
                // Remove from pending calls
                self.pending_calls.retain(|c| c != &call);
                Ok(Some(call))
            }
            _ => Ok(None),
        }
    }

    /// Add a new hall call to the system
    pub fn add_hall_call(&mut self, call: HallCall) {
        self.pending_calls.push(call);
    }

    /// Check if the dispatch algorithm has converged
    pub fn has_converged(&self) -> bool {
        self.cbaa.has_converged()
    }

    /// Get the currently assigned call (if any)
    pub fn get_assigned_call(&self) -> Option<HallCall> {
        self.cbaa.get_assignment()
    }
}

fn main() -> Result<(), AuctionError> {
    println!("Elevator Dispatch System Example");
    println!("================================");

    // This is a library example showing the structure
    // In a real implementation, you would:
    // 1. Set up actual network communication
    // 2. Create multiple elevator controllers
    // 3. Run the dispatch loop continuously
    // 4. Handle real hall calls from building systems

    println!("This example demonstrates the elevator dispatch system structure.");
    println!("See the source code for implementation details.");
    println!("Run 'cargo run --example simple_auction' for a working example.");

    Ok(())
}
