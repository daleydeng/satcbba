# Distributed CBBA Simulation with DDS

This project supports a distributed mode where each satellite runs as a separate process (Agent) and communicates via DDS. A central Server coordinates the simulation rhythm and visualizes results.

## Prerequisites

- Rust toolchain
- DDS implementation (RustDDS is embedded, no external service needed for local discovery usually, but multicast must be enabled on your network interface).

## Components

1.  **DDS Server (`examples/dds_server.rs`)**:
    - Loads tasks and satellite definitions.
    - Publishes tasks to agents.
    - Sends synchronization signals (Start, Iteration).
    - Collects agent states and prints summary.

2.  **DDS Agent (`examples/dds_agent.rs`)**:
    - Runs on each satellite (simulated).
    - Connects to DDS domain.
    - Listens for tasks and control signals.
    - Performs CBBA logic (Bundle Construction, Consensus).
    - Reports state to Server.

## How to Run

1.  **Start the Server**:
    ```bash
    cargo run --example dds_server
    ```
    The server will wait for 5 seconds for agents to join, then start the simulation.

2.  **Start Agents** (in separate terminals):
    You need to start one agent process for each satellite ID (1 to 6 based on default data).
    
    ```bash
    cargo run --example dds_agent -- 1
    cargo run --example dds_agent -- 2
    cargo run --example dds_agent -- 3
    cargo run --example dds_agent -- 4
    cargo run --example dds_agent -- 5
    cargo run --example dds_agent -- 6
    ```

    *Tip: On Windows PowerShell, you can use `start` to launch new windows:*
    ```powershell
    start cargo run --example dds_agent -- 1
    start cargo run --example dds_agent -- 2
    ...
    ```

## Architecture

- **Communication**:
    - **Tasks**: Server -> `NewTask` Topic -> Agents
    - **Control**: Server -> `ServerControl` Topic -> Agents (Start/Stop/Reset)
    - **Consensus**: Agents <-> `ConsensusMessage` Topic <-> Agents (Bid exchange)
    - **Sync**: Agents -> `SyncMessage` Topic -> Server (Status report)
    - **State**: Agents -> `AgentState` Topic -> Server (Visualization data)

- **Async/Tokio**:
    - Agents use `tokio` runtime.
    - DDS readers run in spawned async tasks and communicate with the main loop via channels.
    - The main loop handles logic sequentially but non-blocking.
