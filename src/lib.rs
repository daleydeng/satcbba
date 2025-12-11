//! # satcbba - CBAA/CBBA Library for Rust
//!
//! satcbba is a Rust implementation of the Consensus-Based Auction Algorithm (CBAA)
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
//! - [`dds`] - DDS communication implementation
//!
//! ## Quick Start
//!
//! ```rust
//! use satcbba::*;
//! use serde::{Serialize, Deserialize};
//!
//! // Define your types
//! #[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
//! struct Task(u32);
//! impl satcbba::Task for Task {
//!     fn id(&self) -> satcbba::TaskId { satcbba::TaskId(self.0) }
//! }
//!
//! #[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize, PartialOrd, Ord)]
//! struct Agent(u32);
//! impl satcbba::Agent for Agent {
//!     fn id(&self) -> satcbba::AgentId { satcbba::AgentId(self.0) }
//! }
//!
//! // Implement score function
//! struct SimpleScoreFunction;
//!
//! impl ConstraintChecker<Task, Agent> for SimpleScoreFunction {
//!     fn check(&self, _agent: &Agent, _task: &Task) -> bool {
//!         true
//!     }
//! }
//!
//! impl satcbba::cbaa::ScoreFunction<Task, Agent> for SimpleScoreFunction {
//!     fn calc(&self, _agent: &Agent, task: &Task) -> Score {
//!         Score(100 - task.0)
//!     }
//! }
//!
//! impl satcbba::cbba::ScoreFunction<Task, Agent> for SimpleScoreFunction {
//!     fn calc(&self, _agent: &Agent, task: &Task, _path: &[Task], _position: usize) -> Score {
//!         Score(100 - task.0)
//!     }
//! }
//! }
//!
//! // Create and use CBAA
//! let mut cbaa = CBAA::new(Agent(1), SimpleScoreFunction, vec![Task(1), Task(2)], None);
//! let result = cbaa.auction_phase();
//! ```

#![allow(ambiguous_glob_reexports)]

pub mod config;
pub mod consensus;
pub mod dds;
pub mod error;
pub mod logger;
pub mod sat;

pub use consensus::*;
pub use dds::*;
pub use error::*;
pub use sat::*;
