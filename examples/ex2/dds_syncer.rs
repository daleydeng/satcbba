//! Syncer for Distributed CBBA
//!
//! Usage:
//! cargo run --example dds_syncer
//!
//! This syncer:
//! 1. Synchronizes agent phases (Bundle Construction -> Consensus -> ...)
//! 2. Collects agent status
//! 3. Reports system status to Manager

use cbbadds::{
    dds::{
        AgentPhase, ManagerCommand,
        RequestFromSyncer, ReplyToManager, create_common_qos,
        RequestFromManager, ReplyToSyncer
    },
    sat::{ExploreTask, load_satellites},
};
use cbbadds::consensus::types::AgentId;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use clap::Parser;
use cbbadds::config::load_pkl;
use futures::StreamExt;
use rustdds::with_key::Sample;

#[path = "config.rs"]
pub mod config;
use config::Config;

#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path)?;
    run(config).await
}


use config::SatSourceConfig;

pub async fn run(config: Config) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting DDS Syncer...");

    // --- Load Data (for verification only) ---
    let satellites = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => {
             cbbadds::sat::generate_random_satellites(cfg)
        },
        SatSourceConfig::File(cfg) => {
            load_satellites(Path::new(&cfg.path))?
        }
    };

    let _expected_agents: HashSet<AgentId> = satellites.iter().map(|s| s.id).collect();
    let expected_count = config.dds.expected_agents;

    println!("Configuration expects {} agents.", expected_count);

    // --- DDS Setup ---
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();
    let (network_writer, network_reader) = cbbadds::dds::transport::new_syncer_transport::<ExploreTask>(domain_id, qos);
    println!("Syncer Network Initialized");

    // --- Create Streams ---
    let mut manager_stream = network_reader.manager_request_stream;
    let mut agent_status_stream = network_reader.agent_status_stream;
    let mut reply_to_syncer_stream = network_reader.reply_to_syncer_stream;

    // --- Wait for Agents ---
    println!("Waiting for agents to come online... (Expecting {} agents)", expected_count);
    let mut ready_agents = HashSet::new();

    // Loop until all agents are seen or Manager says Start (which might be risky if agents aren't ready)
    // Actually, we should wait for agents to be "Initializing"

    // We can also wait for a "Start" command from Manager to begin the actual sync loop
    let _simulation_started = false;

    // --- Main Event Loop ---
    let mut current_iteration = 0;
    let mut current_phase = AgentPhase::Initializing;
    let mut waiting_for_completion = false;
    let mut current_request_id: Option<String> = None;

    let mut peer_replies: HashMap<AgentId, ReplyToSyncer> = HashMap::new();

    loop {
        tokio::select! {
            // Handle Agent Status Updates
            Some(result) = agent_status_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };

                let agent_id = msg.agent_id;
                let phase = msg.phase;

                 // Track new agents
                 if phase == AgentPhase::Initialized
                    && ready_agents.insert(agent_id) {
                        println!("Agent {} is ready. ({}/{})", agent_id, ready_agents.len(), expected_count);

                        // Report status to Manager
                        let reply = ReplyToManager {
                            request_id: current_request_id.clone().unwrap_or_default(),
                            success: true,
                            message: "Agent Ready".to_string(),
                            iteration: Some(current_iteration),
                            phase: Some(current_phase),
                            active_agents: Some(ready_agents.len() as u32),
                            converged: None,
                        };
                        network_writer.publish_reply_to_manager(reply)?;
                    }
            }

            // Handle Agent Replies (Task Completion)
            Some(result) = reply_to_syncer_stream.next() => {
                 let Ok(sample) = result else { continue };
                 let Sample::Value(msg) = sample.into_value() else { continue };

                // Track agents from replies if not already known (implicit registration)
                // In a real system, we'd want explicit registration, but for this simulation,
                // we can infer presence from activity if needed, or rely on expected_count.
                // Since we removed status stream, we don't see "Initializing" status anymore.
                // We should assume agents are ready if they are replying, or just wait for expected_count of replies.
                // However, "ready_agents" set is empty now because we removed the filling logic.
                // We need to populate it.

                ready_agents.insert(msg.agent_id);

                // Check if this reply matches current request
                let Some(req_id) = &current_request_id else { continue; };

                if &msg.request_id != req_id {
                    continue;
                }
                peer_replies.insert(msg.agent_id, msg.clone());

                // Check completion
                if !waiting_for_completion {
                    continue;
                }

                let all_done = ready_agents.iter().all(|id| peer_replies.contains_key(id));

                if !all_done {
                    continue;
                }
                println!("Step finished: Iteration {} Phase {:?}", current_iteration, current_phase);
                waiting_for_completion = false;

                // Check Convergence
                // Check has_changes based on phase payload
                let any_changes = peer_replies.values().any(|r| {
                    match r.phase {
                        AgentPhase::BundleConstructionComplete(has_changes) => has_changes,
                        AgentPhase::ConflictResolutionComplete(has_changes) => has_changes,
                        _ => false,
                    }
                });

                // Notify Manager
                let reply = ReplyToManager {
                    request_id: current_request_id.clone().unwrap_or_default(),
                    success: true,
                    message: "Phase Finished".to_string(),
                    iteration: Some(current_iteration),
                    phase: Some(current_phase),
                    active_agents: Some(ready_agents.len() as u32),
                    converged: Some(!any_changes),
                };
                network_writer.publish_reply_to_manager(reply)?;

                // Clear replies for next step
                peer_replies.clear();
            }

            // Handle Manager Commands
            Some(result) = manager_stream.next() => {
                let Ok(sample) = result else { continue };
                let msg = sample.into_value();

                match msg {
                    RequestFromManager::Command { id, command } => {
                        current_request_id = Some(id.clone());
                        match command {
                            ManagerCommand::StartStep(instr) => {
                                println!("Manager requested Step: Phase {:?}", instr.target_phase);
                                // current_iteration = instr.iteration; // AgentInstruction no longer has iteration
                                current_phase = match instr.target_phase {
                                    cbbadds::dds::AgentRequestPhase::Initializing => cbbadds::dds::AgentPhase::Initializing,
                                    cbbadds::dds::AgentRequestPhase::BundleConstruction => {
                                        current_iteration += 1;
                                        cbbadds::dds::AgentPhase::BundleConstruction
                                    },
                                    cbbadds::dds::AgentRequestPhase::ConflictResolution => cbbadds::dds::AgentPhase::ConflictResolution,
                                };
                                waiting_for_completion = true;
                                peer_replies.clear(); // Clear old replies

                                let _ = network_writer.publish_syncer_request(RequestFromSyncer::Instruction(instr));
                            },
                            ManagerCommand::Stop => {
                                println!("Manager requested Stop. Stopping.");
                                break;
                            },
                            ManagerCommand::Reset => {
                                println!("Manager requested Reset.");
                                current_iteration = 0;
                                current_phase = AgentPhase::Initializing;
                                waiting_for_completion = false;
                                peer_replies.clear();
                            },
                            ManagerCommand::ClearTasks => {
                                println!("Manager requested ClearTasks.");
                                let msg = RequestFromSyncer::ClearTasks;
                                if let Err(e) = network_writer.publish_syncer_request(msg) {
                                        eprintln!("Error broadcasting ClearTasks: {:?}", e);
                                }
                            },
                            ManagerCommand::Shutdown => {
                                println!("Manager requested Shutdown. Exiting.");
                                break;
                            },
                            _ => {}
                        }
                    }
                    RequestFromManager::DistributeTasks { id: _, tasks } => {
                            // Broadcast tasks to Agents
                            let msg = RequestFromSyncer::ForwardTasks(tasks);
                            if let Err(e) = network_writer.publish_syncer_request(msg) {
                                eprintln!("Error broadcasting tasks: {:?}", e);
                            }
                    }
                }
            }
        }
    }

    Ok(())
}
