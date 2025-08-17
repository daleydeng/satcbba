//! Error types for auction operations

use std::fmt;

/// Errors that can occur during auction operations
#[derive(Debug, Clone, PartialEq)]
pub enum AuctionError {
    /// Invalid task or agent identifier
    InvalidIdentifier,
    /// Network communication error
    NetworkError(String),
    /// Consensus phase failed
    ConsensusFailed,
    /// Invalid bid value
    InvalidBid,
    /// Configuration error
    ConfigurationError(String),
    /// Algorithm state error
    StateError(String),
}

impl fmt::Display for AuctionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AuctionError::InvalidIdentifier => write!(f, "Invalid task or agent identifier"),
            AuctionError::NetworkError(msg) => write!(f, "Network error: {}", msg),
            AuctionError::ConsensusFailed => write!(f, "Consensus phase failed"),
            AuctionError::InvalidBid => write!(f, "Invalid bid value"),
            AuctionError::ConfigurationError(msg) => write!(f, "Configuration error: {}", msg),
            AuctionError::StateError(msg) => write!(f, "State error: {}", msg),
        }
    }
}

impl std::error::Error for AuctionError {}
