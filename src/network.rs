//! Network communication abstractions

use crate::error::AuctionError;
use crate::types::{AgentId, Task};
use std::collections::HashMap;

/// Message types for consensus communication
///
/// These messages are exchanged between agents during the consensus phase
/// to coordinate winning bids and resolve conflicts.
#[derive(Debug, Clone, PartialEq)]
pub enum ConsensusMessage<A: AgentId, T: Task> {
    /// Winning bids list update
    WinningBids { agent_id: A, bids: HashMap<T, f64> },
    /// Winning agents list update  
    WinningAgents {
        agent_id: A,
        assignments: HashMap<T, A>,
    },
    /// Timestamp update for conflict resolution
    Timestamp {
        agent_id: A,
        timestamps: HashMap<A, u64>,
    },
}

/// Trait for network communication handlers
///
/// Implement this trait to provide network communication capabilities
/// for the auction algorithms. This abstraction allows the library
/// to work with different network protocols and topologies.
pub trait NetworkHandler<A: AgentId, T: Task> {
    /// Send a consensus message to neighbors
    ///
    /// # Arguments
    ///
    /// * `message` - The consensus message to send
    ///
    /// # Returns
    ///
    /// Ok(()) on success, or an AuctionError on failure
    fn send_message(&mut self, message: ConsensusMessage<A, T>) -> Result<(), AuctionError>;

    /// Receive pending consensus messages
    ///
    /// This method should return all pending messages without blocking.
    /// If no messages are available, return an empty vector.
    ///
    /// # Returns
    ///
    /// A vector of consensus messages, or an AuctionError on failure
    fn receive_messages(&mut self) -> Result<Vec<ConsensusMessage<A, T>>, AuctionError>;

    /// Get list of neighboring agent IDs
    ///
    /// # Returns
    ///
    /// A vector of agent IDs that are direct neighbors in the network
    fn get_neighbors(&self) -> Vec<A>;
}
