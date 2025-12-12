//! Distributed CBBA Agent using DDS for communication and synchronization
//!
//! Usage:
//! cargo run --example dds_agent -- <agent_id>
//!
//! The agent listens for tasks and control signals from the syncer,
//! and communicates with other agents for consensus.

use anyhow::{Result, anyhow};
use clap::Parser;
use futures::StreamExt;
use rustdds::with_key::Sample;
use satcbba::CBBA;
use satcbba::consensus::types::{AgentId, ConsensusMessage};
use satcbba::dds::transport::AgentWriter;
use satcbba::dds::{AgentCommand, AgentCommandPayload, AgentPhase, AgentReply, create_common_qos};
use satcbba::sat::score::SatelliteScoreFunction;
use satcbba::sat::types::{ExploreTask, Satellite};
use tracing::{debug, info, warn};
use tracing_subscriber;

// Configuration file no longer needed for the agent; domain_id is passed via CLI.

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Agent ID
    agent_id: u32,

    /// DDS domain id
    #[arg(long, default_value_t = 0)]
    domain_id: u16,
}

struct AgentContext {
    iteration: usize,
    consensus_buffer: Vec<ConsensusMessage>,
    pending_tasks: Option<Vec<ExploreTask>>,
    cbba: Option<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
}

impl AgentContext {
    fn new() -> Self {
        Self {
            iteration: 0,
            consensus_buffer: Vec::new(),
            pending_tasks: None,
            cbba: None,
        }
    }

    fn reset(&mut self) {
        self.iteration = 0;
        self.consensus_buffer.clear();
        self.pending_tasks = None;
        if let Some(cbba) = self.cbba.as_mut() {
            cbba.clear();
        }
    }
}

fn should_handle_command(agent_id: AgentId, recipients: &Option<Vec<AgentId>>) -> bool {
    match recipients {
        None => true, // broadcast
        Some(list) => list.is_empty() || list.iter().any(|id| id == &agent_id),
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,rustdds=error,rustdds::rtps=error,rustdds::network::udp_sender=error",
                )
            }),
        )
        .try_init();

    let cli = Cli::parse();
    let agent_id = cli.agent_id;
    let domain_id = cli.domain_id;

    let _role_guard = tracing::info_span!("Agent", id = agent_id).entered();

    run(agent_id, domain_id).await
}

pub async fn run(agent_id: u32, domain_id: u16) -> Result<()> {
    let agent_identifier = AgentId(agent_id);

    info!(
        "Starting Agent {} (Async/Tokio); waiting for initialization from syncer",
        agent_identifier.0
    );

    // --- DDS Setup ---
    let qos = create_common_qos();

    // Get Writer and Reader parts
    let (network_writer, network_reader) = satcbba::dds::transport::new_agent_transport::<
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
        ExploreTask,
    >(agent_identifier, domain_id, qos);
    debug!("Agent {} transport initialized", agent_identifier.0);

    // --- Create Streams ---
    let mut syncer_stream = network_reader.syncer_request_stream;
    let mut consensus_stream = network_reader.peer_consensus_stream;

    // --- CBBA Setup ---
    network_writer.publish_status(AgentPhase::Standby)?;

    let mut ctx = AgentContext::new();

    // --- Main Loop ---
    loop {
        tokio::select! {
            Some(result) = syncer_stream.next() => {
                match result {
                    Ok(sample) => {
                        let msg = sample.into_value();
                            handle_syncer_message(msg, agent_identifier, &mut ctx, &network_writer)
                        .await?;
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


fn ensure_cbba_present(
    ctx: &AgentContext,
    request_id: &str,
    agent_id: AgentId,
    network_writer: &AgentWriter<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
) -> Result<bool> {
    if ctx.cbba.is_some() {
        return Ok(true);
    }

    warn!("Received command before initialization; replying standby");
    network_writer.publish_status(AgentPhase::Standby)?;
    let reply = AgentReply {
        request_id: request_id.to_string(),
        agent_id,
        iteration: ctx.iteration,
        phase: AgentPhase::Standby,
    };
    network_writer.publish_reply_to_syncer(reply)?;
    Ok(false)
}

async fn handle_syncer_message(
    msg: AgentCommand<
        ExploreTask,
        ConsensusMessage,
        CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    >,
    agent_id: satcbba::consensus::types::AgentId,
    ctx: &mut AgentContext,
    network_writer: &AgentWriter<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>,
) -> Result<()> {
    let AgentCommand {
        request_id,
        recipients,
        payload,
    } = msg;

    if !should_handle_command(agent_id, &recipients) {
        return Ok(());
    }

    match payload {
        AgentCommandPayload::Handshake => {
            ctx.reset();
            info!("Agent {} handshake acknowledged", agent_id.0);
            network_writer.publish_status(AgentPhase::Connected)?;
            let reply = AgentReply {
                request_id,
                agent_id,
                iteration: ctx.iteration,
                phase: AgentPhase::Connected,
            };
            network_writer.publish_reply_to_syncer(reply)?;
        }
        AgentCommandPayload::Initialization { state } => {
            ctx.reset();
            info!(
                "Agent {} received Initialization req {} targeting agent {}",
                agent_id.0,
                request_id,
                state.agent.id.0
            );
            // Agent id mismatch is a programming/config error; fail fast.
            assert_eq!(
                state.agent.id, agent_id,
                "Initialization state sent to wrong agent: expected {:?}, got {:?}",
                agent_id, state.agent.id
            );

            network_writer.publish_status(AgentPhase::Initializing)?;
            info!(
                "Initialized agent {} at pos ({}, {})",
                agent_id.0,
                state.agent.lat_e6,
                state.agent.lon_e6
            );
            ctx.cbba = Some(state);

            network_writer.publish_status(AgentPhase::Initialized)?;

            let reply = AgentReply {
                request_id,
                agent_id,
                iteration: ctx.iteration,
                phase: AgentPhase::Initialized,
            };
            network_writer.publish_reply_to_syncer(reply)?;
        }
        AgentCommandPayload::BundlingConstruction { tasks } => {
            if !ensure_cbba_present(ctx, &request_id, agent_id, network_writer)? {
                return Ok(());
            }
            let cbba = ctx.cbba.as_mut().expect("cbba checked present");
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
            network_writer.publish_status(AgentPhase::BundlingConstructingComplete(added))?;
        }
        AgentCommandPayload::ConflictResolution { messages } => {
            if !ensure_cbba_present(ctx, &request_id, agent_id, network_writer)? {
                return Ok(());
            }
            let cbba = ctx.cbba.as_mut().expect("cbba checked present");
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

            let buffered_messages = std::mem::take(&mut ctx.consensus_buffer);

            info!(
                "Processing {} buffered consensus messages",
                buffered_messages.len()
            );
            let dropped_tasks = cbba.consensus_phase(&buffered_messages);

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
            network_writer.publish_status(AgentPhase::ConflictResolvingComplete(dropped_tasks))?;
        }
        AgentCommandPayload::Terminate => {
            if !ensure_cbba_present(ctx, &request_id, agent_id, network_writer)? {
                return Ok(());
            }
            let cbba = ctx.cbba.as_ref().expect("cbba checked present");
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
        AgentCommandPayload::ReadyCheck => {
            let phase = if ctx.cbba.is_some() {
                AgentPhase::Initialized
            } else {
                AgentPhase::Standby
            };
            network_writer.publish_status(phase.clone())?;
            let reply = AgentReply {
                request_id,
                agent_id,
                iteration: ctx.iteration,
                phase,
            };
            network_writer.publish_reply_to_syncer(reply)?;
        }
    }
    Ok(())
}
