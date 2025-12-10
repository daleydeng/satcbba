//! Distributed CBBA Agent using DDS for communication and synchronization
//!
//! Usage:
//! cargo run --example dds_agent -- <agent_id>
//!
//! The agent listens for tasks and control signals from the syncer,
//! and communicates with other agents for consensus.

use anyhow::{Context, Result, anyhow};
use cbbadds::CBBA;
use cbbadds::config::load_pkl;
use cbbadds::consensus::types::ConsensusMessage;
use cbbadds::dds::transport::AgentWriter;
use cbbadds::dds::{AgentCommand, AgentPhase, AgentReply, create_common_qos};
use cbbadds::sat::load_satellites;
use cbbadds::sat::score::SatelliteScoreFunction;
use cbbadds::sat::types::{ExploreTask, Satellite};
use clap::Parser;
use futures::StreamExt;
use rustdds::with_key::Sample;
use std::path::Path;
use tracing::{debug, info, warn};
use tracing_subscriber;

#[path = "config.rs"]
pub mod config;
use config::Config;
use config::SatSourceConfig;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Agent ID
    agent_id: u32,

    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,
}

struct AgentContext {
    iteration: usize,
    consensus_buffer: Vec<ConsensusMessage>,
    pending_tasks: Option<Vec<ExploreTask>>,
}

impl AgentContext {
    fn new() -> Self {
        Self {
            iteration: 0,
            consensus_buffer: Vec::new(),
            pending_tasks: None,
        }
    }

    fn reset(&mut self) {
        self.iteration = 0;
        self.consensus_buffer.clear();
        self.pending_tasks = None;
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,rustdds=warn,rustdds::rtps=warn")
            }),
        )
        .try_init();

    let cli = Cli::parse();
    let agent_id = cli.agent_id;
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path).context("Failed to load configuration")?;
    run(agent_id, config).await
}

pub async fn run(agent_id: u32, config: Config) -> Result<()> {
    // Load satellite configuration
    let satellites = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => cbbadds::sat::generate_random_satellites(cfg),
        SatSourceConfig::File(cfg) => {
            load_satellites(Path::new(&cfg.path)).context("Failed to load satellites from file")?
        }
    };

    let agent = satellites
        .iter()
        .find(|s| s.id.0 == agent_id)
        .cloned()
        .ok_or_else(|| anyhow!("Agent ID {} not found in configuration", agent_id))?;

    info!(
        "Starting Agent {} (Async/Tokio) at pos ({}, {})",
        agent_id, agent.lat_e6, agent.lon_e6
    );

    // --- DDS Setup ---
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();

    // Get Writer and Reader parts
    let (network_writer, network_reader) = cbbadds::dds::transport::new_agent_transport::<
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
        ExploreTask,
    >(agent.id, domain_id, qos);
    debug!("Agent {} transport initialized", agent_id);

    // --- Create Streams ---
    let mut syncer_stream = network_reader.syncer_request_stream;
    let mut consensus_stream = network_reader.peer_consensus_stream;

    // --- CBBA Setup ---
    let config_cbba = config.algo.cbba.clone();
    let score_function = SatelliteScoreFunction;
    let mut cbba = CBBA::new(agent, score_function, Vec::new(), Some(config_cbba));

    network_writer.publish_status(AgentPhase::Standby)?;

    let mut ctx = AgentContext::new();

    // --- Main Loop ---
    loop {
        tokio::select! {
            Some(result) = syncer_stream.next() => {
                match result {
                    Ok(sample) => {
                        let msg = sample.into_value();
                        handle_syncer_message(msg, &mut ctx, &mut cbba, &network_writer).await?;
                    }
                    Err(e) => warn!("Failed to read syncer request: {:?}", e),
                }
            }
            Some(result) = consensus_stream.next() => {
                match result {
                    Ok(sample) => {
                        if let Sample::Value(msg) = sample.into_value() {
                            debug!("Buffered consensus message from {:?}", msg.agent_id);
                            ctx.consensus_buffer.push(msg);
                        }
                    }
                    Err(e) => warn!("Failed to read consensus message: {:?}", e),
                }
            }
        }
    }
}

async fn handle_syncer_message(
    msg: AgentCommand<ExploreTask, ConsensusMessage, CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
    ctx: &mut AgentContext,
    cbba: &mut CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    network_writer: &AgentWriter<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
) -> Result<()> {
    match msg {
        AgentCommand::Initialization { request_id, state } => {
            network_writer.publish_status(AgentPhase::Initializing)?;
            ctx.reset();
            if let Some(initial_state) = state {
                *cbba = initial_state;
            } else {
                cbba.clear();
            }
            network_writer.publish_status(AgentPhase::Initialized)?;

            let reply = AgentReply {
                request_id,
                agent_id: cbba.agent.id,
                iteration: ctx.iteration,
                phase: AgentPhase::Initialized,
            };
            network_writer.publish_reply_to_syncer(reply)?;
        }
        AgentCommand::BundlingConstruction { request_id, tasks } => {
            ctx.iteration += 1;
            network_writer.publish_status(AgentPhase::BundlingConstructing)?;
            info!(
                "Starting Bundle Construction Phase (iteration {})",
                ctx.iteration
            );

            if let Some(new_tasks) = tasks {
                info!("Applying {} tasks from syncer", new_tasks.len());
                ctx.pending_tasks = Some(new_tasks);
            }

            let pending = ctx.pending_tasks.take();
            let added = cbba
                .bundle_construction_phase(pending.as_deref())
                .map_err(|e| anyhow!("Bundle construction failed: {e}"))?;
            info!(
                "Bundle Construction finished (iteration {}), added {} tasks",
                ctx.iteration,
                added.len()
            );

            if !added.is_empty() {
                network_writer.publish_state(cbba.clone())?;
                network_writer.publish_consensus_message(cbba.consensus_message())?;
            }

            let reply = AgentReply {
                request_id,
                agent_id: cbba.agent.id,
                iteration: ctx.iteration,
                phase: AgentPhase::BundlingConstructingComplete(added.clone()),
            };
            network_writer.publish_reply_to_syncer(reply)?;
            network_writer
                .publish_status(AgentPhase::BundlingConstructingComplete(added))?;
        }
        AgentCommand::ConflictResolution { request_id, messages } => {
            network_writer.publish_status(AgentPhase::ConflictResolving)?;
            info!(
                "Starting Conflict Resolution Phase (iteration {})",
                ctx.iteration
            );

            if let Some(inline_messages) = messages {
                debug!(
                    "Merging {} consensus messages supplied by syncer",
                    inline_messages.len()
                );
                ctx.consensus_buffer.extend(inline_messages);
            }

            info!(
                "Processing {} buffered consensus messages",
                ctx.consensus_buffer.len()
            );
            let dropped_tasks = cbba.consensus_phase(&ctx.consensus_buffer);
            ctx.consensus_buffer.clear();

            let has_changes = !dropped_tasks.is_empty();
            if has_changes {
                network_writer.publish_state(cbba.clone())?;
            }

            let reply = AgentReply {
                request_id,
                agent_id: cbba.agent.id,
                iteration: ctx.iteration,
                phase: AgentPhase::ConflictResolvingComplete(dropped_tasks.clone()),
            };
            network_writer.publish_reply_to_syncer(reply)?;
            network_writer
                .publish_status(AgentPhase::ConflictResolvingComplete(dropped_tasks))?;
        }
        AgentCommand::Terminate { request_id } => {
            network_writer.publish_status(AgentPhase::Terminating)?;

            let reply = AgentReply {
                request_id,
                agent_id: cbba.agent.id,
                iteration: ctx.iteration,
                phase: AgentPhase::Terminating,
            };
            network_writer.publish_reply_to_syncer(reply)?;

            network_writer.publish_status(AgentPhase::Standby)?;
            return Err(anyhow!("Received terminate command from syncer"));
        }
    }
    Ok(())
}
