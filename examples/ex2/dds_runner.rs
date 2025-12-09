//! Runner for Distributed CBBA Example
//!
//! Usage:
//! cargo run --example dds_runner
//!
//! This runner spawns separate processes for:
//! 1. One Syncer process
//! 2. N Agent processes
//! 3. One Manager process
//!
//! It simulates a full deployment on a single machine using multiple processes.

use std::process::Command;
use std::thread;
use std::time::Duration;
use clap::Parser;
use cbbadds::config::load_pkl;

// We need a dummy config struct just to read num_agents
#[derive(serde::Deserialize)]
struct DdsConfig {
    num_agents: usize,
}
#[derive(serde::Deserialize)]
struct Config {
    dds: DdsConfig,
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path)?;

    println!("Starting DDS Runner (Multi-Process) with config from {}", config_path);
    println!("Spawning {} Agents, 1 Syncer, and 1 Manager...", config.dds.num_agents);

    let mut children = Vec::new();

    // 1. Spawn Syncer
    println!("Runner: Spawning Syncer...");
    let syncer = Command::new("cargo")
        .args(&["run", "--example", "dds_syncer", "--", "--config", config_path])
        .env("RUST_LOG", "warn,rustdds=info") // Enable DDS logs
        .env("RUSTDDS_INTERFACE", "127.0.0.1") // Force Loopback
        .spawn()
        .expect("Failed to spawn syncer");
    children.push(("Syncer", syncer));

    // Wait for syncer to initialize
    thread::sleep(Duration::from_secs(3));

    // 2. Spawn Agents
    for id in 1..=config.dds.num_agents {
        println!("Runner: Spawning Agent {}...", id);

        // Using environment variables to control port assignment in RustDDS (if supported)
        // or passing domain_id offset if that helps isolate traffic (though they need to communicate).
        // RustDDS doesn't have a direct "port" env var for discovery, it relies on DomainID + ParticipantID.
        // But we can try to stagger them or just let them find each other on loopback.
        // The issue with 10048 (AddrInUse) is usually due to multicast port conflict on Windows.
        // We can try to force unicast only if we could configure it, but for now we rely on
        // retry or ignoring the bind error if it's just multicast.

        let agent = Command::new("cargo")
            .args(&["run", "--example", "dds_agent", "--", &id.to_string(), "--config", config_path])
            .env("RUST_LOG", "warn,rustdds=info")
            .env("RUSTDDS_INTERFACE", "127.0.0.1")
            .spawn()
            .expect("Failed to spawn agent");
        children.push(("Agent", agent));

        // Stagger start
        thread::sleep(Duration::from_millis(1000));
    }

    // Wait for agents to settle
    thread::sleep(Duration::from_secs(2));

    // 3. Spawn Manager
    println!("Runner: Spawning Manager...");
    let manager = Command::new("cargo")
        .args(&["run", "--example", "dds_manager", "--", "--config", config_path])
        .env("RUST_LOG", "warn,rustdds=info")
        .env("RUSTDDS_INTERFACE", "127.0.0.1")
        .spawn()
        .expect("Failed to spawn manager");
    children.push(("Manager", manager));

    // Monitor children
    loop {
        let mut all_finished = true;
        for (name, child) in children.iter_mut() {
             match child.try_wait() {
                 Ok(Some(status)) => {
                     println!("Process {} finished with status: {}", name, status);
                 }
                 Ok(None) => {
                     all_finished = false;
                 }
                 Err(e) => {
                     eprintln!("Error waiting for {}: {}", name, e);
                 }
             }
        }

        if all_finished {
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }

    println!("Runner finished.");
    Ok(())
}
