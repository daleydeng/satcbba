//! Distributed CBBA Agent using DDS for communication and synchronization
//!
//! Usage:
//! cargo run --example dds_agent -- <agent_id>
//!
//! The agent listens for tasks and control signals from the syncer,
//! and communicates with other agents for consensus.

use cbbadds::CBBA;
use cbbadds::consensus::types::ConsensusMessage;
use cbbadds::dds::{
    AgentPhase, AgentRequestPhase,  create_common_qos,
    RequestFromSyncer, ReplyToSyncer
};
use cbbadds::sat::score::SatelliteScoreFunction;
use cbbadds::sat::types::{Satellite, ExploreTask};
use cbbadds::sat::load_satellites;
use std::path::Path;
use cbbadds::config::load_pkl;
use futures::StreamExt;
use rustdds::with_key::Sample;

#[path = "config.rs"]
pub mod config;
use config::Config;
use config::{SatSourceConfig};

use clap::Parser;
use anyhow::{Result, anyhow, Context};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Agent ID
    agent_id: u32,

    /// Path to configuration file
    #[arg(long, default_value = "examples/ex2/config.pkl")]
    config: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let agent_id = cli.agent_id;
    let config_path = &cli.config;
    let config: Config = load_pkl(config_path).context("Failed to load configuration")?;
    run(agent_id, config).await
}

use cbbadds::dds::transport::AgentWriter;

struct AgentContext {
    iteration: usize,
    consensus_buffer: Vec<ConsensusMessage>,
}

impl AgentContext {
    fn reset(&mut self) {
        self.iteration = 0;
        self.consensus_buffer.clear();
    }
}

pub async fn run(agent_id: u32, config: Config) -> Result<()> {
    // Load satellite configuration
    let satellites = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => {
             cbbadds::sat::generate_random_satellites(cfg)
        },
        SatSourceConfig::File(cfg) => {
            load_satellites(Path::new(&cfg.path)).context("Failed to load satellites from file")?
        }
    };

    let agent = satellites.iter()
        .find(|s| s.id.0 == agent_id)
        .cloned()
        .ok_or_else(|| anyhow!("Agent ID {} not found in configuration", agent_id))?;

    println!("Starting Agent {} (Async/Tokio) at pos ({}, {})...", agent_id, agent.lat_e6, agent.lon_e6);

    // --- DDS Setup ---
    let domain_id = config.dds.domain_id;
    let qos = create_common_qos();

    // Get Writer and Reader parts
    let (network_writer, network_reader) = cbbadds::dds::transport::new_agent_transport::<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>, ExploreTask>(agent.id, domain_id, qos);
    println!("Agent {} GUID: {:?}", agent_id, "UNKNOWN (Hidden in Handler)");

    // --- Create Streams ---
    let mut syncer_stream = network_reader.syncer_request_stream;
    let mut consensus_stream = network_reader.peer_consensus_stream;

    // Send Initializing Status
    network_writer.publish_status(AgentPhase::Initializing)?;

    // --- CBBA Setup ---
    let config_cbba = config.algo.cbba.clone();
    let score_function = SatelliteScoreFunction;
    let mut cbba = CBBA::new(agent, score_function, Vec::new(), Some(config_cbba));

    network_writer.publish_status(AgentPhase::Initialized)?;

    let mut ctx = AgentContext {
        iteration: 0,
        consensus_buffer: Vec::new(),
    };

    // --- Main Loop ---
    loop {
        tokio::select! {
            // Handle Syncer Requests
            Some(result) = syncer_stream.next() => {
                 if let Ok(sample) = result {
                     let msg = sample.into_value();
                     handle_syncer_message(msg, &mut ctx, &mut cbba, &network_writer).await?;
                 }
            }

            // Handle Consensus Messages
            Some(result) = consensus_stream.next() => {
                if let Ok(sample) = result
                && let Sample::Value(msg) = sample.into_value() {
                    // Buffer message
                    // println!("Buffered consensus message from {:?}", msg.agent_id);
                    ctx.consensus_buffer.push(msg);
                }
            }
        }
    }
}

async fn handle_syncer_message(
    msg: RequestFromSyncer<ExploreTask>,
    ctx: &mut AgentContext,
    cbba: &mut CBBA<ExploreTask, Satellite, SatelliteScoreFunction>,
    network_writer: &AgentWriter<CBBA<ExploreTask, Satellite, SatelliteScoreFunction>>
) -> Result<()> {
    match msg {
        RequestFromSyncer::Instruction(instr) => {
            let req_id = instr.request_id.clone();

            match instr.target_phase {
                AgentRequestPhase::Initializing => {
                    network_writer.publish_status(AgentPhase::Initializing)?;
                    ctx.reset();
                    cbba.reset();
                    network_writer.publish_status(AgentPhase::Initialized)?;
                },
                AgentRequestPhase::BundleConstruction => {
                    // New iteration starts with Bundle Construction
                    ctx.iteration += 1;

                    println!("Starting Bundle Construction Phase...");
                    let added = cbba.bundle_construction_phase().unwrap_or_default();
                    let has_changes = !added.is_empty();
                    println!("Bundle Construction done. Added: {}", added.len());


                    // Publish State
                    if has_changes {
                        network_writer.publish_state(cbba.clone())?;

                        network_writer.publish_consensus_message(ConsensusMessage {
                            agent_id: cbba.agent.id,
                            bids: cbba.bids.clone(),
                        })?;
                    }

                    // Reply to Syncer
                    let reply = ReplyToSyncer {
                        request_id: req_id.clone(),
                        agent_id: cbba.agent.id,
                        iteration: ctx.iteration,
                        phase: AgentPhase::BundleConstructionComplete(has_changes),
                    };
                    network_writer.publish_reply_to_syncer(reply)?;
                    // Also publish status (for backward compatibility or redundancy if needed)
                    network_writer.publish_status(AgentPhase::BundleConstructionComplete(has_changes))?;
                },
                AgentRequestPhase::ConflictResolution => {
                    println!("Starting Conflict Resolution Phase...");

                    // Process buffered consensus messages
                    println!("Processing {} buffered consensus messages", ctx.consensus_buffer.len());
                    let dropped_tasks = cbba.consensus_phase(&ctx.consensus_buffer)?;
                    ctx.consensus_buffer.clear();

                    let has_changes = !dropped_tasks.is_empty();
                    if has_changes {
                        network_writer.publish_state(cbba.clone())?;
                    }

                    // Reply to Syncer
                    let reply = ReplyToSyncer {
                        request_id: req_id.clone(),
                        agent_id: cbba.agent.id,
                        iteration: ctx.iteration,
                        phase: AgentPhase::ConflictResolutionComplete(has_changes),
                    };
                    network_writer.publish_reply_to_syncer(reply)?;
                    network_writer.publish_status(AgentPhase::ConflictResolutionComplete(has_changes))?;
                },
            }
        },
        RequestFromSyncer::ForwardTasks(tasks) => {
            for t in tasks {
                println!("Received new task: T{}", t.id);
                cbba.add_task(t);
            }
        }
        RequestFromSyncer::ClearTasks => {
            println!("Clearing tasks...");
            cbba.reset();
            ctx.reset();
        }
    }
    Ok(())
}
