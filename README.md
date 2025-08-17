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
In medieval Scandinavian law texts, *laga-kefli* referred to a law-staff Ñ a symbol of authority and decision-making.  

This is a fitting metaphor for a library that provides consensus-based auction algorithms:  
just as the *kefli* represented authority and resolution in legal assemblies, the algorithms here  
serve as the mechanism for reaching agreement and assigning tasks in distributed multi-agent systems.

## Features

- **CBAA**: Single-assignment consensus-based auctions
- **CBBA**: Multi-assignment consensus-based bundle auctions  
- **Generic Design**: Works with any task and agent types
- **Configurable**: Flexible cost functions and network topologies
- **Async-Ready**: Designed to work with async/await patterns
- **No Dependencies**: Lightweight core with no external dependencies

## Installation

Add this to your `Cargo.toml`:

```toml
[dependencies]
kefli = "0.1.0"
```

## Quick Start

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
    println!("Agent {:?} assigned to task {:?}", agent_id, assigned_task);
}
```

## Library Structure

The library is organized into several modules:

### Core Modules

- **`types`** - Core types and traits (`Task`, `AgentId`, `CostFunction`, etc.)
- **`error`** - Error types and handling
- **`config`** - Configuration structures for algorithms
- **`network`** - Network communication abstractions
- **`cbaa`** - Single-assignment CBAA implementation
- **`cbba`** - Multi-assignment CBBA implementation

### Key Types

- `CBAA<T, A, C>` - Single-assignment algorithm
- `CBBA<T, A, C>` - Multi-assignment algorithm  
- `AuctionConfig` - Configuration parameters
- `ConsensusMessage<A, T>` - Messages for consensus
- `NetworkHandler<A, T>` - Trait for network communication

## Examples

The library includes several examples:

### Simple Auction

```rust
// Run with: cargo run --example simple_auction
// Shows basic CBAA usage with delivery tasks
```

### Elevator Dispatch System

```rust  
// Run with: cargo run --example elevator_system
// Comprehensive P2P elevator dispatch implementation
```

## Algorithm Background

The algorithms are based on the paper:

> Choi, H. L., Brunet, L., & How, J. P. (2009). Consensus-based decentralized auctions for robust task allocation. *IEEE Transactions on Robotics*, 25(4), 912-926.

### CBAA (Single Assignment)

1. **Auction Phase**: Agents bid on tasks with highest reward
2. **Consensus Phase**: Exchange winning bids and resolve conflicts  
3. **Convergence**: Repeat until stable assignment reached

### CBBA (Multi Assignment)

1. **Bundle Construction**: Build bundle of tasks greedily
2. **Conflict Resolution**: Handle complex multi-task conflicts
3. **Convergence**: Account for task dependencies

## Configuration

Configure algorithms via `AuctionConfig`:

```rust
use std::time::Duration;

let config = AuctionConfig::cbba(3)  // Max 3 tasks per agent
    .with_network_diameter(5)        // Network diameter of 5
    .with_timeout(Duration::from_secs(2))  // 2 second timeout
    .with_async_resolution(true);    // Enable async conflict resolution

// Or use convenience methods:
let config = AuctionConfig::cbba(3)
    .with_timeout_secs(2)           // 2 seconds
    .with_timeout_ms(1500);         // 1.5 seconds
```

## Network Integration

Implement `NetworkHandler` for your network:

```rust
impl NetworkHandler<AgentId, TaskType> for MyNetworkHandler {
    fn send_message(&mut self, message: ConsensusMessage<AgentId, TaskType>) -> Result<(), AuctionError> {
        // Send over your network protocol
    }
    
    fn receive_messages(&mut self) -> Result<Vec<ConsensusMessage<AgentId, TaskType>>, AuctionError> {
        // Receive pending messages  
    }
    
    fn get_neighbors(&self) -> Vec<AgentId> {
        // Return neighbor list
    }
}
```

## Performance Guarantees

Both algorithms provide theoretical guarantees:

- **Convergence**: Finite time to conflict-free assignment
- **Optimality**: At least 50% optimal solutions
- **Robustness**: Tolerant to network changes and failures

## Use Cases

- **Robotics**: Multi-robot task allocation and coordination
- **Elevators**: Distributed elevator dispatch (see example)
- **Logistics**: Vehicle routing and delivery optimization
- **Cloud Computing**: Distributed task scheduling
- **IoT**: Sensor network coordination

## Testing

Run tests with:

```bash
cargo test                    # Unit tests
cargo test --test integration # Integration tests
```

## Contributing

Contributions welcome! Please feel free to submit issues and pull requests.
