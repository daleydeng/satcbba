# Kefli - CBAA/CBBA Library for Rust

[![Crates.io](https://img.shields.io/crates/v/kefli.svg)](https://crates.io/crates/kefli)
[![Documentation](https://docs.rs/kefli/badge.svg)](https://docs.rs/kefli)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/ipeglin/kefli)

Kefli is a Rust implementation of the Consensus-Based Auction Algorithm (CBAA) and Consensus-Based Bundle Algorithm (CBBA) for distributed task allocation. These algorithms are particularly useful in multi-agent systems where autonomous agents need to coordinate and agree on task assignments without centralized control.

## Acknowledgment

The algorithms implemented in this crate were originally developed and published by:

> H.-L. Choi, L. Brunet, and J. P. How,
> *Consensus-Based Decentralized Auctions for Robust Task Allocation,*
> **IEEE Transactions on Robotics**, vol. 25, no. 4, pp. 912-926, Aug. 2009.
> [DOI: 10.1109/TRO.2009.2022423](https://doi.org/10.1109/TRO.2009.2022423) | [IEEE Xplore Link](https://ieeexplore.ieee.org/document/5072249)

This crate is an independent, from-scratch reimplementation of the algorithms in Rust.
It does **not** redistribute or copy any of the original article's text, figures, or code; only the algorithmic ideas have been re-expressed.
If you use this crate in academic work, please cite the original paper above.

## Name

The crate is named **Kefli** after the Old Norse word *kefli*, meaning a wooden staff, stick, or cylinder.
In medieval Scandinavian law texts, *laga-kefli* referred to a law-staff — a symbol of authority and decision-making.

This is a fitting metaphor for a library that provides consensus-based auction algorithms:
just as the *kefli* represented authority and resolution in legal assemblies, the algorithms here
serve as the mechanism for reaching agreement and assigning tasks in distributed multi-agent systems.

## Features

- **CBAA**: Single-assignment consensus-based auctions
- **CBBA**: Multi-assignment consensus-based bundle auctions
- **Generic Design**: Works with any task and agent types
- **Configurable**: Flexible cost functions and network topologies
- **Async-Ready**: Designed to work with async/await patterns
- **Lightweight core:** optional dependencies only

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
kefli = "0.1.0"
```

## Quick Start

### CBAA (Single Assignment)

```rust
use kefli::*;

// Define your task and agent types
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Task(u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
struct Agent(u32);

// Implement a cost function
struct SimpleCostFunction;

impl CostFunction<Task, Agent> for SimpleCostFunction {
    fn calculate_cost(&self, agent: Agent, task: &Task, _context: &AssignmentContext<Task, Agent>) -> f64 {
        // Simple example: prefer lower task IDs
        100.0 - task.0 as f64
    }
}

// Create and run CBAA
let agent_id = Agent(1);
let cost_function = SimpleCostFunction;
let available_tasks = vec![Task(1), Task(2), Task(3)];

let mut cbaa = CBAA::new(agent_id, cost_function, available_tasks, None);

// Phase 1: Auction
let result = cbaa.auction_phase()?;

// Phase 2: Consensus (process messages from network)
let messages = vec![]; // Get from your network handler
cbaa.consensus_phase(messages)?;

// Get result
if let Some(assigned_task) = cbaa.get_assignment() {
    println!("Agent {} assigned to task {}", agent_id, assigned_task);
}
```

### CBBA (Multi-Assignment)

```rust
use cbbadds::*;
use cbbadds::cbba::ScoreFunction;
use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MultiTask(u32);
impl Task for MultiTask {
    fn id(&self) -> TaskId { TaskId(self.0) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize, PartialOrd, Ord)]
struct MultiAgent(u32);
impl Agent for MultiAgent {
    fn id(&self) -> AgentId { AgentId(self.0) }
}

struct MultiScoreFunction;

impl ConstraintChecker<MultiTask, MultiAgent> for MultiScoreFunction {
    fn check(&self, _agent: &MultiAgent, _task: &MultiTask) -> bool {
        true
    }
}

impl ScoreFunction<MultiTask, MultiAgent> for MultiScoreFunction {
    fn calc(&self, _agent: &MultiAgent, task: &MultiTask, _path: &[MultiTask], _position: usize) -> Score {
        Score(task.0)
    }
}

let config = AuctionConfig::cbba(3); // Allow up to 3 tasks per agent
let mut cbba = CBBA::new(
    MultiAgent(1),
    MultiScoreFunction,
    vec![MultiTask(1), MultiTask(2), MultiTask(3)],
    Some(config)
);

let added_tasks = cbba.bundle_construction_phase();
```

## Library Structure

The library is organized into several modules:

### Core Modules

- **`types`** - Core types and traits (`Task`, `AgentId`, `CostFunction`, etc.)
- **`error`** - Error types and handling
- **`config`** - Configuration structures for algorithms
- **`network`** - Network communication abstractions
