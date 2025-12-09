//! Manager for Distributed CBBA
//!
//! Usage:
//! cargo run --example dds_manager
//!
//! This manager:
//! 1. Spawns other components (Server, Agents)
//! 2. Distributes tasks via DDS
//! 3. Monitors system status via DDS
//! 4. Controls simulation lifecycle (Start, Stop, Reset)
//! 5. Visualizes results

// Use modules from other example files
// We need to allow dead_code because main() in these modules won't be used
use std::time::Duration;
use cbbadds::config::load_pkl;
use clap::Parser;

#[allow(dead_code)]
#[path = "config.rs"]
pub mod config;
use config::Config;
use std::collections::HashMap;
use std::path::Path;

use cbbadds::{
    dds::{
        ManagerCommand, RequestFromManager, ReplyToManager, create_common_qos, AgentInstruction
    },
    sat::{ExploreTask, Satellite, visualize_iteration, load_tasks},
    CBBA,
    sat::score::SatelliteScoreFunction,
};
use tokio::sync::mpsc;
use futures::StreamExt;
use rustdds::with_key::Sample;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::builder().filter_level(log::LevelFilter::Warn).init();

    let cli = Cli::parse();
    let config_path = &cli.config;
    let config: Config = match load_pkl(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load pkl config {}: {}", config_path, e);
            return Err(e.into());
        }
    };

    println!("Starting DDS Manager with config from {}", config_path);

    // --- Manager Logic ---
    run_manager(config).await?;

    println!("Manager finished.");
    Ok(())
}

use config::TaskSourceConfig;

pub async fn run_manager(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("Initializing Manager Network...");
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();
    let (network_writer, network_reader) = cbbadds::dds::transport::new_manager_transport::<ExploreTask, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>(domain_id, qos);

    // --- Create Streams ---
    let mut reply_stream = network_reader.reply_to_manager_stream;
    let mut agent_state_stream = network_reader.agent_state_stream;
    let mut log_stream = network_reader.log_stream;

    // We don't necessarily need channels internally if we process inline,
    // but the Manager logic below is structured as a linear script (steps 1, 2, 3...)
    // listening to events.
    // To adapt the linear script to the stream-based API, we can either:
    // 1. Rewrite logic to be purely event-driven (select loop)
    // 2. Use a background task to pump streams into channels (re-implementing what we removed from Transport)

    // Given the structure, option 2 is easier to maintain the existing flow logic.
    // Or we can just interact with streams directly in the linear flow, but that's hard because we need to check multiple streams.

    // Let's create channels to decouple the linear logic from the streams
    let (reply_tx, mut reply_rx) = mpsc::channel::<ReplyToManager>(100);
    let (agent_state_tx, mut agent_state_rx) = mpsc::channel::<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>(100);

    // Spawn a "pump" task
    tokio::spawn(async move {
        loop {
            tokio::select! {
                Some(result) = reply_stream.next() => {
                    let Ok(sample) = result else { continue };
                    let msg = sample.into_value();
                    let _ = reply_tx.send(msg).await;
                }
                Some(result) = agent_state_stream.next() => {
                    let Ok(sample) = result else { continue };
                    if let Sample::Value(msg) = sample.into_value() {
                         let _ = agent_state_tx.send(msg).await;
                    }
                }
                Some(_log) = log_stream.next() => {
                    // Ignore logs for now or print them
                }
            }
        }
    });

    // --- Load Tasks ---
    let tasks = match &config.data.tasks {
        TaskSourceConfig::Random(cfg) => {
             cbbadds::sat::generate_random_tasks(cfg)
        },
        TaskSourceConfig::File(cfg) => {
            load_tasks(Path::new(&cfg.path))?
        }
    };
    println!("Loaded {} tasks to distribute.", tasks.len());

    // --- Output Config ---
    let output_dir = Path::new(&config.output_config.dir);
    if !output_dir.exists() {
        std::fs::create_dir_all(output_dir)?;
    }

    // --- Main Loop ---
    // 1. Wait for Syncer to be ready
    println!("Waiting for Syncer...");
    loop {
        if let Some(reply) = reply_rx.recv().await
            && let Some(active_agents) = reply.active_agents {
                println!("Syncer is online. Active Agents: {}", active_agents);
                if active_agents >= config.dds.expected_agents as u32 {
                    println!("All expected agents are active.");
                    break;
                }
            }
    }

    // Clear Tasks before start
    println!("Manager: Clearing tasks...");
    let req = RequestFromManager::Command {
        id: uuid::Uuid::new_v4().to_string(),
        command: ManagerCommand::ClearTasks,
    };
    network_writer.publish_command(req)?;

    // 2. Distribute Tasks
    println!("Manager: Distributing {} tasks...", tasks.len());

    let id = uuid::Uuid::new_v4().to_string();
    let msg = RequestFromManager::DistributeTasks {
        id,
        tasks: tasks.clone(),
    };
    network_writer.publish_command(msg)?;
    println!("Tasks distributed.");

    // 3. Start Simulation
    println!("Starting Simulation Sequence...");

    // We run the loop here
    let mut iteration = 1;
    let max_iterations = 100;

    'simulation: loop {
        if iteration > max_iterations {
            println!("Max iterations reached.");
            break;
        }

        println!("--- Starting Iteration {} ---", iteration);

        // --- Phase 1: Bundle Construction ---
        println!("Manager: Triggering Bundle Construction...");
        let req = RequestFromManager::Command {
            id: uuid::Uuid::new_v4().to_string(),
            command: ManagerCommand::StartStep(AgentInstruction {
                request_id: uuid::Uuid::new_v4().to_string(),
                target_phase: cbbadds::dds::AgentRequestPhase::BundleConstruction
            })
        };
        network_writer.publish_command(req)?;

        // Wait for completion
        let mut phase1_converged = false;
        loop {
            match reply_rx.recv().await {
                Some(reply) => {
                    if let Some(phase) = reply.phase {
                         // Check if phase matches (ignoring has_changes payload)
                         let is_target_phase = match phase {
                            cbbadds::dds::AgentPhase::BundleConstructionComplete(_) => true,
                            _ => false
                         };

                         if is_target_phase && reply.converged.is_some() {
                             let converged = reply.converged.unwrap();
                             println!("Manager: Bundle Construction Finished. Converged: {}", converged);
                             phase1_converged = converged;
                             break;
                         }
                    }
                     // Optional: Print status
                     if let Some(active) = reply.active_agents {
                         // println!("Status: {}/{} agents active", active, config.dds.expected_agents);
                     }
                }
                None => {
                    eprintln!("Manager: Reply channel closed unexpectedly.");
                    break 'simulation;
                }
            }
        }

        // --- Phase 2: Conflict Resolution ---
        println!("Manager: Triggering Conflict Resolution...");
        let req = RequestFromManager::Command {
             id: uuid::Uuid::new_v4().to_string(),
             command: ManagerCommand::StartStep(AgentInstruction {
                request_id: uuid::Uuid::new_v4().to_string(),
                target_phase: cbbadds::dds::AgentRequestPhase::ConflictResolution
            })
        };
        network_writer.publish_command(req)?;

        // Wait for completion
        let mut phase2_converged = false;
        loop {
            match reply_rx.recv().await {
                Some(reply) => {
                    if let Some(phase) = reply.phase {
                         // Check if phase matches (ignoring has_changes payload)
                         let is_target_phase = match phase {
                            cbbadds::dds::AgentPhase::ConflictResolutionComplete(_) => true,
                            _ => false
                         };

                         if is_target_phase && reply.converged.is_some() {
                             let converged = reply.converged.unwrap();
                             println!("Manager: Conflict Resolution Finished. Converged: {}", converged);
                             phase2_converged = converged;
                             break;
                         }
                    }
                }
                None => break 'simulation,
            }
        }

        // --- Check Convergence ---
        // If no tasks added (phase1) and no conflict updates (phase2), we are done.
        // Actually, usually if no conflict updates, it means we are stable.
        // But strictly, if bundle construction added nothing, AND conflict resolution changed nothing, we are done.

        if phase1_converged && phase2_converged {
            println!("System Converged at Iteration {}!", iteration);
            break;
        }

        iteration += 1;
        tokio::time::sleep(Duration::from_millis(100)).await; // Small pause
    }

    // 4. Collect Final States (Optional, or wait a bit)
    println!("Simulation Ended. Collecting final states...");
    tokio::time::sleep(Duration::from_secs(2)).await;

    // We need to drain the state channel to get latest states
    let mut agent_states: HashMap<String, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>> = HashMap::new();
    while let Ok(state) = agent_state_rx.try_recv() {
        agent_states.insert(state.agent.id.to_string(), state);
    }

    // Also wait a bit more or request state dump?
    // Agents send state on BundleConstructionComplete. So we should have them.

    // Save final visualization
    if !agent_states.is_empty() {
         let cbba_instances: Vec<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>> = agent_states
            .values()
            .cloned()
            .collect();

        let viz_options = config.viz.clone();
        if let Err(e) = visualize_iteration(999, &cbba_instances, &tasks, output_dir, &viz_options) {
            eprintln!("Visualization error: {}", e);
        } else {
            println!("Saved final visualization.");
        }
    } else {
        println!("No agent states received.");
    }

    // Stop everyone
    println!("Sending STOP command...");
    let req = RequestFromManager::Command {
        id: uuid::Uuid::new_v4().to_string(),
        command: ManagerCommand::Stop,
    };
    network_writer.publish_command(req)?;

    Ok(())
}
