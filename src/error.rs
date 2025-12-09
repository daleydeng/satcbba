//! Error types for auction operations

use thiserror::Error;

/// Errors that can occur during auction operations
#[derive(Error, Debug, Clone, PartialEq)]
pub enum Error {
    #[error("Invalid task or agent identifier")]
    InvalidIdentifier,
    #[error("Network error: {0}")]
    NetworkError(String),
    #[error("Consensus phase failed")]
    ConsensusFailed,
    #[error("Invalid bid value")]
    InvalidBid,
    #[error("Configuration error: {0}")]
    ConfigurationError(String),
    #[error("State error: {0}")]
    StateError(String),
    #[error("Validation error: {0}")]
    ValidationError(String),
    #[error("Task not found: {0}")]
    TaskNotFound(String),
    #[error("Item already exists")]
    ItemAlreadyExists,
    #[error("Capacity full")]
    CapacityFull,
    #[error("Index out of bounds")]
    IndexOutOfBounds,
}

/// A specialized Result type for auction operations
pub type Result<T> = std::result::Result<T, Error>;
