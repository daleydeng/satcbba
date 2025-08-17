// SPDX-License-Identifier: MIT OR Apache-2.0
// Copyright (c) 2025 Ian Philip Eglin
//! Configuration structures for auction algorithms

use std::time::Duration;

/// Configuration for auction algorithms
///
/// This struct contains parameters that control the behavior of the
/// CBAA and CBBA algorithms.
#[derive(Debug, Clone)]
pub struct AuctionConfig {
    /// Maximum number of tasks per agent (Lt in paper)
    ///
    /// For CBAA, this should be 1. For CBBA, this can be larger.
    pub max_tasks_per_agent: usize,

    /// Network diameter for convergence calculations (D in paper)
    ///
    /// This represents the maximum number of hops between any two agents
    /// in the communication network.
    pub network_diameter: usize,

    /// Timeout for consensus operations
    ///
    /// How long to wait for consensus messages before timing out.
    pub consensus_timeout: Duration,

    /// Enable asynchronous conflict resolution
    ///
    /// When true, agents can process consensus messages asynchronously.
    /// When false, synchronous conflict resolution is used.
    pub async_conflict_resolution: bool,
}

impl Default for AuctionConfig {
    fn default() -> Self {
        Self {
            max_tasks_per_agent: 1,
            network_diameter: 3,
            consensus_timeout: Duration::from_secs(5),
            async_conflict_resolution: true,
        }
    }
}

impl AuctionConfig {
    /// Create a new configuration for CBAA (single assignment)
    pub fn cbaa() -> Self {
        Self {
            max_tasks_per_agent: 1,
            ..Default::default()
        }
    }

    /// Create a new configuration for CBBA (multi assignment)
    pub fn cbba(max_tasks: usize) -> Self {
        Self {
            max_tasks_per_agent: max_tasks,
            ..Default::default()
        }
    }

    /// Set the maximum number of tasks per agent
    pub fn with_max_tasks(mut self, max_tasks: usize) -> Self {
        self.max_tasks_per_agent = max_tasks;
        self
    }

    /// Set the network diameter
    pub fn with_network_diameter(mut self, diameter: usize) -> Self {
        self.network_diameter = diameter;
        self
    }

    /// Set the consensus timeout
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.consensus_timeout = timeout;
        self
    }

    /// Set the consensus timeout in milliseconds (convenience method)
    pub fn with_timeout_ms(mut self, timeout_ms: u64) -> Self {
        self.consensus_timeout = Duration::from_millis(timeout_ms);
        self
    }

    /// Set the consensus timeout in seconds (convenience method)  
    pub fn with_timeout_secs(mut self, timeout_secs: u64) -> Self {
        self.consensus_timeout = Duration::from_secs(timeout_secs);
        self
    }

    /// Enable or disable asynchronous conflict resolution
    pub fn with_async_resolution(mut self, async_resolution: bool) -> Self {
        self.async_conflict_resolution = async_resolution;
        self
    }
}
