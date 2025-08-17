// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2024 Ian Philip Eglin
//! # Kefli - CBAA/CBBA Library for Rust
//!
//! Kefli is a Rust implementation of the Consensus-Based Auction Algorithm (CBAA)
//! and Consensus-Based Bundle Algorithm (CBBA) for distributed task allocation.
//!
//! ## Modules
//!
//! - [`cbaa`] - Single-assignment consensus-based auction algorithm
//! - [`cbba`] - Multi-assignment consensus-based bundle algorithm  
//! - [`types`] - Core types and traits
//! - [`error`] - Error types and handling
//! - [`config`] - Configuration structures
//! - [`network`] - Network communication abstractions
//!
//! ## Quick Start
//!
//! ```rust
//! use kefli::*;
//!
//! // Define your types
//! #[derive(Debug, Clone, PartialEq, Eq, Hash)]
//! struct Task(u32);
//!
//! #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
//! struct Agent(u32);
//!
//! // Implement cost function
//! struct SimpleCostFunction;
//!
//! impl CostFunction<Task, Agent> for SimpleCostFunction {
//!     fn calculate_cost(&self, _agent: Agent, task: &Task, _context: &AssignmentContext<Task, Agent>) -> f64 {
//!         100.0 - task.0 as f64
//!     }
//! }
//!
//! // Create and use CBAA
//! let mut cbaa = CBAA::new(Agent(1), SimpleCostFunction, vec![Task(1), Task(2)], None);
//! let result = cbaa.auction_phase();
//! ```

pub mod cbaa;
pub mod cbba;
pub mod config;
pub mod error;
pub mod network;
pub mod types;

// Re-export main types for convenience
pub use cbaa::CBAA;
pub use cbba::CBBA;
pub use config::*;
pub use error::*;
pub use network::*;
pub use types::*;
