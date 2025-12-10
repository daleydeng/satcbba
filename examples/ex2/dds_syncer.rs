//! Syncer for Distributed CBBA
//!
//! Usage:
//! cargo run --example dds_syncer
//!
//! This syncer:
//! 1. Synchronizes agent phases (Bundle Construction -> Consensus -> ...)
//! 2. Collects agent status
//! 3. Reports system status to Manager

use cbbadds::config::load_pkl;
use cbbadds::consensus::types::AgentId;
use cbbadds::sat::score::SatelliteScoreFunction;
use cbbadds::sat::types::Satellite;
use cbbadds::CBBA;
use cbbadds::{
    dds::{AgentCommand, AgentPhase, AgentReply, AgentStatus, create_common_qos},
    sat::{
        ExploreTask, generate_random_tasks, load_satellites, load_tasks, render_visualization,
        save_satellites, save_tasks,
    },
};
use chrono::Local;
use clap::Parser;
use futures::StreamExt;
use rustdds::with_key::Sample;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::time::{Duration, Instant};
use uuid::Uuid;

#[path = "config.rs"]
pub mod config;
use config::{Config, SatSourceConfig, TaskSourceConfig};

#[derive(Parser, Debug, Default)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,

    /// When converged, send terminate command to agents
    #[arg(long, default_value_t = false)]
    terminate_agents: bool,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path)?;
    run(config, cli.terminate_agents).await
}

pub async fn run(
    config: Config,
    terminate_agents_on_convergence: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Starting DDS Syncer...");

    // --- Prepare output directory ---
    let output_dir = &config.output_config.dir;
    let result_dir = if config.output_config.use_timestamp {
        let date_str = Local::now()
            .format(&config.output_config.timestamp_fmt)
            .to_string();
        Path::new(output_dir).join(date_str)
    } else {
        PathBuf::from(output_dir)
    };
    fs::create_dir_all(&result_dir)?;
    println!("Results will be saved to: {}", result_dir.display());

    // --- Load Satellites (for verification/output) ---
    let _satellites = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => {
            let sats = cbbadds::sat::generate_random_satellites(cfg);
            let path = result_dir.join("satellites.json");
            if let Err(e) = save_satellites(&sats, &path) {
                println!("Warning: failed to save satellites: {e}");
            }
            sats
        }
        SatSourceConfig::File(cfg) => {
            let path = Path::new(&cfg.path);
            let dest = result_dir.join("satellites.json");
            if let Err(e) = fs::copy(path, &dest) {
                println!(
                    "Warning: failed to copy satellites file {} -> {}: {e}",
                    path.display(),
                    dest.display()
                );
            }
            load_satellites(path)?
        }
    };

    println!("Waiting for agents to come online via DDS discovery...");

    // --- DDS Setup ---
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();
    let (network_writer, network_reader) = cbbadds::dds::transport::new_syncer_transport::<
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
        ExploreTask,
    >(domain_id, qos);
    println!("Syncer Network Initialized");

    // --- Create Streams ---
    let mut agent_status_stream = network_reader.agent_status_stream;
    let mut agent_state_stream = network_reader.agent_state_stream;
    let mut reply_to_syncer_stream = network_reader.reply_to_syncer_stream;
    let mut latest_states: HashMap<AgentId, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>> =
        HashMap::new();

    // --- Wait for Agents ---
    println!("Waiting for agents to respond to handshake...");
    let mut ready_agents = HashSet::new();
    run_handshake(
        &mut ready_agents,
        &mut agent_status_stream,
        &mut reply_to_syncer_stream,
        &network_writer,
        Duration::from_millis(config.dds.handshake_timeout_ms),
    )
    .await?;

    // --- Load Tasks (shared across iterations) ---
    let tasks = match &config.data.tasks {
        TaskSourceConfig::Random(cfg) => {
            println!("Generating {} random tasks", cfg.count);
            let tasks = generate_random_tasks(cfg);
            let path = result_dir.join("tasks.json");
            if let Err(e) = save_tasks(&tasks, &path) {
                println!("Warning: failed to save tasks: {e}");
            }
            tasks
        }
        TaskSourceConfig::File(cfg) => {
            println!("Loading tasks from file: {}", cfg.path);
            let path = Path::new(&cfg.path);
            let dest = result_dir.join("tasks.json");
            if let Err(e) = fs::copy(path, &dest) {
                println!(
                    "Warning: failed to copy tasks file {} -> {}: {e}",
                    path.display(),
                    dest.display()
                );
            }
            load_tasks(path)?
        }
    };

    if tasks.is_empty() {
        println!("No tasks available; exiting.");
        return Ok(());
    }

    // --- Iterative CBBA coordination over DDS ---
    let max_iterations = config.dds.max_iterations;
    let phase_timeout = Duration::from_millis(config.dds.handshake_timeout_ms);
    let mut iteration = 0;
    let mut converged = false;

    while iteration < max_iterations && !converged {
        iteration += 1;
        println!("--- Iteration {} ---", iteration);

        let participants: HashSet<AgentId> = ready_agents.clone();
        if participants.is_empty() {
            println!("No participating agents; stopping iterations.");
            break;
        }

        // Phase 1: Bundle Construction
        let bundle_request_id = format!("bundle-{}-{}", iteration, Uuid::new_v4());
        network_writer.publish_syncer_request(AgentCommand::BundlingConstruction {
            request_id: bundle_request_id.clone(),
            tasks: Some(tasks.clone()),
        })
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        let bundle_deadline = Instant::now() + phase_timeout;
        let bundle_replies = collect_phase_replies(
            &bundle_request_id,
            &participants,
            &mut ready_agents,
            &mut agent_status_stream,
            &mut agent_state_stream,
            &mut latest_states,
            &mut reply_to_syncer_stream,
            bundle_deadline,
        )
        .await;

        let bundle_changed = bundle_replies.values().any(|r| match &r.phase {
            AgentPhase::BundlingConstructingComplete(added) => !added.is_empty(),
            _ => false,
        });

        if !latest_states.is_empty() {
            let cbba_states: Vec<_> = latest_states.values().cloned().collect();
            let filename = result_dir.join(format!("iter_{:02}_bundle.png", iteration));
            if let Err(e) = render_visualization(
                &filename,
                &format!("Iteration {} (bundle)", iteration),
                &cbba_states,
                &tasks,
                &config.viz,
            ) {
                println!("Warning: failed to visualize bundle stage: {e}");
            }
        }

        // Phase 2: Conflict Resolution
        let conflict_request_id = format!("conflict-{}-{}", iteration, Uuid::new_v4());
        network_writer.publish_syncer_request(AgentCommand::ConflictResolution {
            request_id: conflict_request_id.clone(),
            messages: None,
        })
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        let conflict_deadline = Instant::now() + phase_timeout;
        let conflict_replies = collect_phase_replies(
            &conflict_request_id,
            &participants,
            &mut ready_agents,
            &mut agent_status_stream,
            &mut agent_state_stream,
            &mut latest_states,
            &mut reply_to_syncer_stream,
            conflict_deadline,
        )
        .await;

        let conflict_changed = conflict_replies.values().any(|r| match &r.phase {
            AgentPhase::ConflictResolvingComplete(released) => !released.is_empty(),
            _ => false,
        });

        if !latest_states.is_empty() {
            let cbba_states: Vec<_> = latest_states.values().cloned().collect();
            let filename = result_dir.join(format!("iter_{:02}_consensus.png", iteration));
            if let Err(e) = render_visualization(
                &filename,
                &format!("Iteration {} (consensus)", iteration),
                &cbba_states,
                &tasks,
                &config.viz,
            ) {
                println!("Warning: failed to visualize consensus stage: {e}");
            }
        }

        let missing_bundle = participants
            .iter()
            .filter(|id| !bundle_replies.contains_key(id))
            .count();
        let missing_conflict = participants
            .iter()
            .filter(|id| !conflict_replies.contains_key(id))
            .count();

        if missing_bundle > 0 || missing_conflict > 0 {
            println!(
                "Iteration {} incomplete: missing {} bundle replies, {} conflict replies.",
                iteration, missing_bundle, missing_conflict
            );
        }

        if !bundle_changed && !conflict_changed && missing_bundle == 0 && missing_conflict == 0 {
            println!("Converged after {} iteration(s).", iteration);
            converged = true;
        }
    }

    if !converged {
        println!("Reached max iterations without convergence.");
    } else if terminate_agents_on_convergence {
        println!("Converged; sending terminate command to agents.");
        let terminate_request_id = format!("terminate-{}", Uuid::new_v4());
        network_writer
            .publish_syncer_request(AgentCommand::Terminate {
                request_id: terminate_request_id.clone(),
            })
            .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

        println!("Awaiting agent termination acknowledgements...");
        await_agent_termination(
            &ready_agents,
            &mut agent_status_stream,
            &mut reply_to_syncer_stream,
            phase_timeout,
            &terminate_request_id,
        )
        .await;
    }

    // Save a visualization of the final assignment if we have states
    if !latest_states.is_empty() {
        let cbba_states: Vec<_> = latest_states.values().cloned().collect();
        let filename = result_dir.join(format!("iteration_{}.png", iteration));
        if let Err(e) = render_visualization(
            &filename,
            &format!("Iteration {}", iteration),
            &cbba_states,
            &tasks,
            &config.viz,
        ) {
            println!("Warning: failed to visualize final iteration: {e}");
        }
    }

    println!("Syncer iterations finished.");

    Ok(())
}

async fn run_handshake(
    ready_agents: &mut HashSet<AgentId>,
    agent_status_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    network_writer: &cbbadds::dds::transport::SyncerWriter<
        ExploreTask,
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    >,
    timeout: Duration,
) -> Result<(), Box<dyn std::error::Error>> {
    let handshake_request_id = format!("sync-init-{}", Uuid::new_v4());
    println!(
        "Broadcasting initialization request (timeout {:?}) to discover agents...",
        timeout
    );

    network_writer
        .publish_syncer_request(AgentCommand::Initialization {
            request_id: handshake_request_id.clone(),
            state: None,
        })
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;

    let deadline = Instant::now() + timeout;
    loop {
        if Instant::now() >= deadline {
            break;
        }

        let sleep = tokio::time::sleep_until(deadline);
        tokio::pin!(sleep);

        tokio::select! {
            _ = &mut sleep => {
                break;
            }
            Some(result) = reply_to_syncer_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                if msg.request_id == handshake_request_id {
                    if ready_agents.insert(msg.agent_id) {
                        println!(
                            "Agent {} responded to handshake. Discovered {} agent(s).",
                            msg.agent_id,
                            ready_agents.len()
                        );
                    }
                } else {
                    let _ = ready_agents.insert(msg.agent_id);
                    println!(
                        "Received reply for request {} during handshake; agent {} marked as present ({} discovered).",
                        msg.request_id,
                        msg.agent_id,
                        ready_agents.len()
                    );
                    // For now we simply record the agent as present; main loop will process future actions.
                }
            }
            Some(result) = agent_status_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                if msg.phase == AgentPhase::Initialized && ready_agents.insert(msg.agent_id) {
                    println!(
                        "Agent {} responded to handshake. Discovered {} agent(s).",
                        msg.agent_id,
                        ready_agents.len()
                    );
                }
            }
            else => break,
        }
    }

    if ready_agents.is_empty() {
        println!(
            "Handshake timed out after {:?}; proceeding with zero registered agents.",
            timeout
        );
    } else {
        println!("Handshake complete: {} agent(s) ready.", ready_agents.len());
    }

    Ok(())
}

async fn collect_phase_replies(
    request_id: &str,
    expected_agents: &HashSet<AgentId>,
    ready_agents: &mut HashSet<AgentId>,
    agent_status_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    agent_state_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
    latest_states: &mut HashMap<AgentId, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
    reply_to_syncer_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    deadline: Instant,
) -> HashMap<AgentId, AgentReply> {
    let mut replies = HashMap::new();

    loop {
        if Instant::now() >= deadline {
            break;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());

        tokio::select! {
            _ = tokio::time::sleep(remaining) => {
                break;
            }
            Some(result) = reply_to_syncer_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                ready_agents.insert(msg.agent_id);
                if msg.request_id != request_id {
                    continue;
                }
                replies.insert(msg.agent_id, msg);
                if expected_agents.iter().all(|id| replies.contains_key(id)) {
                    break;
                }
            }
            Some(result) = agent_status_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                ready_agents.insert(msg.agent_id);
            }
            Some(result) = agent_state_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(state) = sample.into_value() else { continue };
                latest_states.insert(state.agent.id, state);
            }
            else => break,
        }
    }

    replies
}

async fn await_agent_termination(
    ready_agents: &HashSet<AgentId>,
    agent_status_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut cbbadds::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    timeout: Duration,
    terminate_request_id: &str,
) {
    if ready_agents.is_empty() {
        println!("No agents to await termination.");
        return;
    }

    let mut terminated: HashSet<AgentId> = HashSet::new();
    let deadline = Instant::now() + timeout;

    loop {
        if terminated.len() == ready_agents.len() {
            println!("All agents reported termination.");
            break;
        }

        if Instant::now() >= deadline {
            let missing = ready_agents.len().saturating_sub(terminated.len());
            println!("Termination wait timed out; missing {missing} agent(s).");
            break;
        }

        let remaining = deadline.saturating_duration_since(Instant::now());

        tokio::select! {
            _ = tokio::time::sleep(remaining) => {
                let missing = ready_agents.len().saturating_sub(terminated.len());
                if missing > 0 {
                    println!("Termination wait timed out; missing {missing} agent(s).");
                }
                break;
            }
            Some(result) = reply_to_syncer_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                if msg.request_id == terminate_request_id {
                    terminated.insert(msg.agent_id);
                }
            }
            Some(result) = agent_status_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                match msg.phase {
                    AgentPhase::Terminating | AgentPhase::Standby => {
                        terminated.insert(msg.agent_id);
                    }
                    _ => {}
                }
            }
            else => break,
        }
    }
}
