//! Syncer for Distributed CBBA
//!
//! Usage:
//! cargo run --example dds_syncer
//!
//! This syncer:
//! 1. Synchronizes agent phases (Bundle Construction -> Consensus -> ...)
//! 2. Collects agent status
//! 3. Reports system status to Manager

use chrono::Local;
use clap::Parser;
use futures::StreamExt;
use rustdds::with_key::Sample;
use satcbba::CBBA;
use satcbba::config::load_pkl;
use satcbba::consensus::types::{Agent, AgentId, Task as TaskTrait};
use satcbba::sat::score::SatelliteScoreFunction;
use satcbba::sat::report::{
    AgentIterationLog, IterationLog, TaskReleaseRecord, build_report, write_report_json,
};
use satcbba::sat::types::Satellite;
use satcbba::{
    dds::{AgentCommand, AgentCommandPayload, AgentPhase, AgentReply, AgentStatus, create_common_qos},
    sat::{
        ExploreTask, generate_random_tasks, load_satellites, load_tasks, render_visualization,
        save_satellites, save_tasks,
    },
};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use tokio::time::{Duration, Instant};
use std::time::Instant as StdInstant;
use tracing_subscriber;
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
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,rustdds=error,rustdds::rtps=error,rustdds::network::udp_sender=error",
                )
            }),
        )
        .try_init();

    // Tag logs with compact role identifier for the syncer.
    let _role_guard = tracing::info_span!("Syncer").entered();

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
    let satellites = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => {
            let sats = satcbba::sat::generate_random_satellites(cfg);
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

    println!("Waiting for agents to come online via DDS discovery...");

    // --- DDS Setup ---
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();
    let (network_writer, network_reader) = satcbba::dds::transport::new_syncer_transport::<
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
    let mut initial_snapshots: Vec<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>> =
        Vec::new();
    let mut bundle_snapshots: Vec<(usize, Vec<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>)> =
        Vec::new();
    let mut consensus_snapshots: Vec<(usize, Vec<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>)> =
        Vec::new();
    let mut iteration_logs: Vec<IterationLog> = Vec::new();
    let sim_start = StdInstant::now();

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

    // --- Broadcast per-agent CBBA state from syncer ---
    // Allow discovery to settle before sending initialization payloads.
    tokio::time::sleep(Duration::from_millis(
        config.dds.init_broadcast_delay_ms,
    ))
    .await;
    let cbba_config = config.algo.cbba.clone();
    let mut init_requests: HashMap<AgentId, String> = HashMap::new();
    for agent_id in &ready_agents {
        if let Some(agent) = satellites.iter().find(|s| &s.id == agent_id) {
            let cbba = CBBA::new(
                agent.clone(),
                SatelliteScoreFunction,
                tasks.clone(),
                Some(cbba_config.clone()),
            );
            initial_snapshots.push(cbba.clone());
            let init_request_id = format!("init-{}-{}", agent_id.0, Uuid::new_v4());
            let init_request_id_for_map = init_request_id.clone();
            println!(
                "Sending initialization to agent {} (state agent {}, tasks={})",
                agent_id.0,
                cbba.agent.id().0,
                cbba.current_tasks.len()
            );
            network_writer
                .publish_syncer_request(AgentCommand {
                    request_id: init_request_id,
                    recipients: Some(vec![*agent_id]),
                    payload: AgentCommandPayload::Initialization { state: cbba },
                })
                .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })?;
            init_requests.insert(*agent_id, init_request_id_for_map);

            // Stagger init sends slightly to avoid bursts before all matches are ready.
            tokio::time::sleep(Duration::from_millis(
                config.dds.init_per_agent_delay_ms,
            ))
            .await;
        } else {
            println!(
                "Warning: no satellite definition found for agent {}",
                agent_id.0
            );
        }
    }

    await_initialization(
        &init_requests,
        &mut agent_status_stream,
        &mut reply_to_syncer_stream,
        Duration::from_millis(config.dds.handshake_timeout_ms),
    )
    .await;

    // Explicit ready probe to ensure agents have applied initialization state.
    await_ready(
        &mut ready_agents,
        &mut agent_status_stream,
        &mut reply_to_syncer_stream,
        &network_writer,
        Duration::from_millis(config.dds.handshake_timeout_ms),
    )
    .await;

    // --- Iterative CBBA coordination over DDS ---
    let max_iterations = config.dds.max_iterations;
    let phase_timeout = Duration::from_millis(config.dds.handshake_timeout_ms);
    let mut iteration = 0;
    let mut converged = false;

    while iteration < max_iterations && !converged {
        iteration += 1;
        println!("--- Iteration {} ---", iteration);

        let iter_start = Instant::now();

        let participants: HashSet<AgentId> = ready_agents.clone();
        if participants.is_empty() {
            println!("No participating agents; stopping iterations.");
            break;
        }

        // Phase 1: Bundle Construction
        let bundle_request_id = format!("bundle-{}-{}", iteration, Uuid::new_v4());
        let bundle_phase_start = Instant::now();
        network_writer
            .publish_syncer_request(AgentCommand {
                request_id: bundle_request_id.clone(),
                recipients: Some(participants.iter().cloned().collect()),
                payload: AgentCommandPayload::BundlingConstruction {
                    tasks: Some(tasks.clone()),
                },
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
        let bundle_stage_ms = bundle_phase_start.elapsed().as_secs_f64() * 1000.0;

        // Snapshot bundle phase states for deferred visualization
        let bundle_snapshot: Vec<_> = participants
            .iter()
            .filter_map(|id| latest_states.get(id).cloned())
            .collect();
        if !bundle_snapshot.is_empty() {
            bundle_snapshots.push((iteration, bundle_snapshot));
        }

        let bundle_changed = bundle_replies.values().any(|r| match &r.phase {
            AgentPhase::BundlingConstructingComplete(added) => !added.is_empty(),
            _ => false,
        });

        // Phase 2: Conflict Resolution
        let conflict_request_id = format!("conflict-{}-{}", iteration, Uuid::new_v4());
        let consensus_phase_start = Instant::now();
        network_writer
            .publish_syncer_request(AgentCommand {
                request_id: conflict_request_id.clone(),
                recipients: Some(participants.iter().cloned().collect()),
                payload: AgentCommandPayload::ConflictResolution { messages: None },
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
        let consensus_stage_ms = consensus_phase_start.elapsed().as_secs_f64() * 1000.0;

        // Snapshot consensus phase states for deferred visualization
        let consensus_snapshot: Vec<_> = participants
            .iter()
            .filter_map(|id| latest_states.get(id).cloned())
            .collect();
        if !consensus_snapshot.is_empty() {
            consensus_snapshots.push((iteration, consensus_snapshot));
        }

        let conflict_changed = conflict_replies.values().any(|r| match &r.phase {
            AgentPhase::ConflictResolvingComplete(released) => !released.is_empty(),
            _ => false,
        });

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

        let iteration_ms = iter_start.elapsed().as_secs_f64() * 1000.0;

        let mut agent_logs = Vec::new();
        for agent_id in &participants {
            if let Some(reply) = bundle_replies.get(agent_id) {
                let added = match &reply.phase {
                    AgentPhase::BundlingConstructingComplete(tasks) =>
                        tasks.iter().map(|t| t.0).collect(),
                    _ => Vec::new(),
                };
                let released = match conflict_replies.get(agent_id) {
                    Some(r) => match &r.phase {
                        AgentPhase::ConflictResolvingComplete(dropped) => dropped
                            .iter()
                            .map(|(task_id, succs)| TaskReleaseRecord {
                                task_id: task_id.0,
                                successors: succs.iter().map(|s| s.0).collect(),
                            })
                            .collect(),
                        _ => Vec::new(),
                    },
                    None => Vec::new(),
                };

                let bundle = latest_states
                    .get(agent_id)
                    .map(|cbba| cbba.bundle.iter().map(|t| t.id().0).collect())
                    .unwrap_or_default();
                let path = latest_states
                    .get(agent_id)
                    .map(|cbba| cbba.path.iter().map(|t| t.id().0).collect())
                    .unwrap_or_default();

                agent_logs.push(AgentIterationLog {
                    agent_id: agent_id.0,
                    added_tasks: added,
                    released_tasks: released,
                    bundle,
                    path,
                });
            }
        }

        iteration_logs.push(IterationLog {
            iteration,
            bundle_stage_ms,
            consensus_stage_ms,
            iteration_ms,
            converged_after_this: converged,
            agents: agent_logs,
        });
    }

    if !converged {
        println!("Reached max iterations without convergence.");
    } else if terminate_agents_on_convergence {
        println!("Converged; sending terminate command to agents.");
        let terminate_request_id = format!("terminate-{}", Uuid::new_v4());
        network_writer
            .publish_syncer_request(AgentCommand {
                request_id: terminate_request_id.clone(),
                recipients: Some(ready_agents.iter().cloned().collect()),
                payload: AgentCommandPayload::Terminate,
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

    // Save report and final visualization
    if let Some(report_states) = if latest_states.is_empty() {
        None
    } else {
        Some(latest_states.values().cloned().collect::<Vec<_>>())
    } {
        let report = build_report(&report_states, &tasks, iteration_logs, sim_start);
        if let Err(e) = write_report_json(&report, &result_dir) {
            println!("Warning: failed to write summary.json: {e}");
        }

        if config.viz.enabled {
            if !initial_snapshots.is_empty() {
                let filename = result_dir.join("iter_00_initial.png");
                if let Err(e) = render_visualization(
                    &filename,
                    "Iteration 0 (initial)",
                    &initial_snapshots,
                    &tasks,
                    &config.viz,
                ) {
                    println!("Warning: failed to visualize initial state: {e}");
                }
            }

            for (iter_idx, states) in &bundle_snapshots {
                let filename = result_dir.join(format!("iter_{:02}_bundle.png", iter_idx));
                if let Err(e) = render_visualization(
                    &filename,
                    &format!("Iteration {} (bundle)", iter_idx),
                    states,
                    &tasks,
                    &config.viz,
                ) {
                    println!("Warning: failed to visualize bundle iteration {}: {e}", iter_idx);
                }
            }

            for (iter_idx, states) in &consensus_snapshots {
                let filename = result_dir.join(format!("iter_{:02}_consensus.png", iter_idx));
                if let Err(e) = render_visualization(
                    &filename,
                    &format!("Iteration {} (consensus)", iter_idx),
                    states,
                    &tasks,
                    &config.viz,
                ) {
                    println!("Warning: failed to visualize consensus iteration {}: {e}", iter_idx);
                }
            }

        }
    }

    println!("Syncer iterations finished.");

    Ok(())
}

async fn run_handshake(
    ready_agents: &mut HashSet<AgentId>,
    agent_status_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    network_writer: &satcbba::dds::transport::SyncerWriter<
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
        .publish_syncer_request(AgentCommand {
            request_id: handshake_request_id.clone(),
            recipients: None,
            payload: AgentCommandPayload::Handshake,
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
                println!(
                    "Handshake observed AgentReply from {} req={} phase={:?}",
                    msg.agent_id,
                    msg.request_id,
                    msg.phase
                );
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
                        "Received non-handshake reply {} from agent {} during handshake ({} discovered).",
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
                println!(
                    "Handshake observed AgentStatus from {} phase={:?}",
                    msg.agent_id,
                    msg.phase
                );
                if matches!(msg.phase, AgentPhase::Initialized | AgentPhase::Connected)
                    && ready_agents.insert(msg.agent_id)
                {
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
        println!(
            "Handshake complete: {} agent(s) ready -> {:?}.",
            ready_agents.len(),
            ready_agents
        );
    }

    Ok(())
}

async fn await_initialization(
    init_requests: &HashMap<AgentId, String>,
    agent_status_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    _timeout: Duration,
) {
    use rustdds::with_key::Sample;

    if init_requests.is_empty() {
        return;
    }

    let mut pending: HashSet<AgentId> = init_requests.keys().cloned().collect();

    println!(
        "Waiting for initialization acknowledgements from {} agent(s)...",
        pending.len()
    );

    while !pending.is_empty() {
        tokio::select! {
            Some(result) = reply_to_syncer_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                if let Some(expected) = init_requests.get(&msg.agent_id) {
                    if &msg.request_id == expected && matches!(msg.phase, AgentPhase::Initialized) {
                        if pending.remove(&msg.agent_id) {
                            println!("Agent {} acknowledged initialization.", msg.agent_id);
                        }
                    }
                }
            }
            Some(result) = agent_status_stream.next() => {
                let Ok(sample) = result else { continue };
                let Sample::Value(msg) = sample.into_value() else { continue };
                if msg.phase == AgentPhase::Initialized {
                    if pending.remove(&msg.agent_id) {
                        println!("Agent {} reported Initialized status.", msg.agent_id);
                    }
                }
            }
        }
    }
}

async fn await_ready(
    agents: &mut HashSet<AgentId>,
    agent_status_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
    network_writer: &satcbba::dds::transport::SyncerWriter<
        ExploreTask,
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    >,
    timeout: Duration,
) {
    use rustdds::with_key::Sample;

    if agents.is_empty() {
        return;
    }

    let mut pending: HashSet<AgentId> = agents.clone();
    let poll_window = if timeout.is_zero() {
        Duration::from_secs(1)
    } else {
        timeout
    };
    let mut attempt: u32 = 0;

    while !pending.is_empty() {
        attempt += 1;

        let ready_request_id = format!("ready-{}-{}", attempt, Uuid::new_v4());
        if let Err(e) = network_writer.publish_syncer_request(AgentCommand {
            request_id: ready_request_id.clone(),
            recipients: Some(pending.iter().cloned().collect()),
            payload: AgentCommandPayload::ReadyCheck,
        }) {
            println!("Warning: failed to send ReadyCheck attempt {}: {e}", attempt);
        }

        println!(
            "Awaiting ready acknowledgements (attempt {}; {} agent(s) pending)...",
            attempt,
            pending.len()
        );

        let deadline = Instant::now() + poll_window;

        loop {
            if pending.is_empty() {
                break;
            }

            let now = Instant::now();
            if now >= deadline {
                break;
            }

            let remaining = deadline.saturating_duration_since(now);

            tokio::select! {
                _ = tokio::time::sleep(remaining) => {
                    break;
                }
                Some(result) = reply_to_syncer_stream.next() => {
                    let Ok(sample) = result else { continue };
                    let Sample::Value(msg) = sample.into_value() else { continue };
                    if msg.request_id == ready_request_id && matches!(msg.phase, AgentPhase::Initialized) {
                        if pending.remove(&msg.agent_id) {
                            println!("Agent {} ready after init.", msg.agent_id);
                        }
                    } else if matches!(msg.phase, AgentPhase::Initialized) {
                        if pending.remove(&msg.agent_id) {
                            println!("Agent {} reported Initialized status during ready check.", msg.agent_id);
                        }
                    }
                }
                Some(result) = agent_status_stream.next() => {
                    let Ok(sample) = result else { continue };
                    let Sample::Value(msg) = sample.into_value() else { continue };
                    if matches!(msg.phase, AgentPhase::Initialized) {
                        if pending.remove(&msg.agent_id) {
                            println!("Agent {} reported Initialized status during ready check.", msg.agent_id);
                        }
                    }
                }
                else => {
                    // Streams ended; avoid tight loop by breaking and retrying publish.
                    break;
                }
            }
        }
    }

    if pending.is_empty() {
        println!("All agents reported ready.");
    } else {
        println!("Ready check aborted early; still missing {:?}.", pending);
    }
}

async fn collect_phase_replies(
    request_id: &str,
    expected_agents: &HashSet<AgentId>,
    ready_agents: &mut HashSet<AgentId>,
    agent_status_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    agent_state_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    >,
    latest_states: &mut HashMap<AgentId, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
    reply_to_syncer_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
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
                if expected_agents.contains(&msg.agent_id) {
                    ready_agents.insert(msg.agent_id);
                }
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
                if expected_agents.contains(&msg.agent_id) {
                    ready_agents.insert(msg.agent_id);
                }
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
    agent_status_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentStatus>,
    reply_to_syncer_stream: &mut satcbba::dds::transport::KeyedDdsDataReaderStream<AgentReply>,
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
