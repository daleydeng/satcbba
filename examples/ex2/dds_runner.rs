//! Runner for Distributed CBBA Example
//!
//! Usage:
//! cargo run --example dds_runner
//!
//! This runner spawns separate processes for:
//! 1. One Syncer process
//! 2. N Agent processes
//!
//! It simulates a full deployment on a single machine using multiple processes.

use cbbadds::config::load_pkl;
use cbbadds::sat::load_satellites;
use clap::Parser;
use std::env;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

#[path = "config.rs"]
mod config;
use config::{Config, SatSourceConfig};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,

    /// When converged, ask syncer to terminate agents
    #[arg(long, default_value_t = true)]
    terminate_agents: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path)?;

    // Derive example binary name prefix from our own executable name (e.g., "ex2_dds_runner" -> "ex2_")
    let exe_name = env::current_exe()
        .ok()
        .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| "dds_runner".to_string());
    let prefix = exe_name
        .strip_suffix("dds_runner")
        .unwrap_or("");
    let syncer_example = format!("{}dds_syncer", prefix);
    let agent_example = format!("{}dds_agent", prefix);

    // Determine agent count from satellite source
    let agent_count = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => cfg.count,
        SatSourceConfig::File(cfg) => load_satellites(Path::new(&cfg.path))?.len(),
    };

    println!(
        "Starting DDS Runner (Multi-Process) with config from {}",
        config_path
    );
    println!("Spawning {} Agents and 1 Syncer...", agent_count);

    let mut children = Vec::new();

    // 1. Spawn Agents first so they are available before syncer starts
    for id in 1..=agent_count {
        println!("Runner: Spawning Agent {}...", id);

        let agent = Command::new("cargo")
            .args(&["run", "--example", &agent_example, "--", &id.to_string(), "--config", config_path])
            .spawn()
            .expect("Failed to spawn agent");
        children.push(("Agent", agent));

        // Stagger start
        thread::sleep(Duration::from_millis(1000));
    }

    // Give agents time to initialize before syncer joins
    thread::sleep(Duration::from_secs(2));

    // 2. Spawn Syncer
    println!("Runner: Spawning Syncer...");
    let mut syncer_cmd = Command::new("cargo");
    syncer_cmd.args(&["run", "--example", &syncer_example, "--", "--config", config_path]);
    if cli.terminate_agents {
        syncer_cmd.arg("--terminate-agents");
    }

    let syncer = syncer_cmd.spawn().expect("Failed to spawn syncer");
    children.push(("Syncer", syncer));

    // Manager removed: not spawning a separate manager process

    // Monitor children until none are left running
    let mut syncer_finished_at: Option<Instant> = None;
    let syncer_grace = Duration::from_secs(5);

    loop {
        // Retain only still-running children; drop finished ones as we go
        children.retain_mut(|(name, child)| match child.try_wait() {
            Ok(Some(status)) => {
                if *name == "Syncer" {
                    syncer_finished_at = Some(Instant::now());
                }
                println!("Process {} finished with status: {}", name, status);
                false
            }
            Ok(None) => true,
            Err(e) => {
                eprintln!("Error waiting for {}: {}", name, e);
                true
            }
        });

        if let Some(finished_at) = syncer_finished_at {
            if !children.is_empty() {
                let elapsed = finished_at.elapsed();
                if elapsed >= syncer_grace {
                    println!(
                        "Syncer finished; terminating remaining {} child(ren) after {:?} grace.",
                        children.len(), syncer_grace
                    );
                    for (name, child) in &mut children {
                        if let Err(e) = child.kill() {
                            eprintln!("Failed to kill {}: {}", name, e);
                        }
                    }
                    for (name, child) in &mut children {
                        if let Err(e) = child.wait() {
                            eprintln!("Failed to wait {}: {}", name, e);
                        }
                    }
                    children.clear();
                }
            }
        }

        if children.is_empty() {
            println!("All child processes finished; runner exiting.");
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }

    println!("Runner finished.");
    Ok(())
}
