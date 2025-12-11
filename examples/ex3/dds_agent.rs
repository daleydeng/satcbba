use cbbadds::consensus::types::AgentId;
use cbbadds::dds::transport::new_agent_transport;
use cbbadds::dds::types::{AgentStatus, Event};
use cbbadds::dds::utils::create_common_qos;
use futures::StreamExt;
use rustdds::StatusEvented;
use serde_json::json;
use std::env;
use tokio;
use tracing::{debug, error, info};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber so `RUST_LOG` controls logging (including rustdds)
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new(
                    "info,rustdds=warn,rustdds::rtps=warn,rustdds::network::udp_sender=error",
                )
            }),
        )
        .try_init();
    let args: Vec<String> = env::args().collect();
    let agent_id: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);

    info!("[Agent {}] Starting Agent...", agent_id);

    let domain_id = 42u16;
    let qos = create_common_qos();

    debug!("[Agent {}] Creating transport...", agent_id);
    let (agent_writer, agent_reader) = new_agent_transport::<AgentStatus, cbbadds::sat::ExploreTask>(
        AgentId(agent_id),
        domain_id,
        qos,
    );
    info!(
        "[Agent {}] Transport created. Listening for SyncerEvents and AgentEvents...",
        agent_id
    );

    // Debug: create writer status stream (rustdds low-level)
    let mut writer_status_stream = agent_writer.agent_event_writer.as_async_status_stream();

    // Sample streams (async_sample_stream) for actual data samples
    let mut syncer_stream = agent_reader.syncer_event_stream;
    let mut agent_stream = agent_reader.peer_agent_event_stream;

    // Event streams (async_event_stream) for reader lifecycle/status events (debug)
    let mut syncer_event_stream = syncer_stream.async_event_stream();
    let mut agent_event_stream = agent_stream.async_event_stream();

    let mut peer_pongs: Vec<String> = Vec::new();

    loop {
        tokio::select! {
            syncer_res = syncer_stream.next() => {
                match syncer_res {
                    Some(Ok(sample)) => {
                        debug!("[Agent {}] Received SyncerEvent sample: {:?}", agent_id, sample);
                        let ev = sample.into_value();
                        debug!("[Agent {}] SyncerEvent value: event_type={}", agent_id, ev.event_type);
                        if ev.event_type == "ping" {
                            // try to extract ping_id from the ping payload so we can echo it back
                            let ping_id = serde_json::from_slice::<serde_json::Value>(&ev.data)
                                .ok()
                                .and_then(|v| v.get("ping_id").and_then(|x| x.as_u64()))
                                .unwrap_or(0);
                            let payload = json!({
                                "agent_id": agent_id,
                                "ping_id": ping_id,
                                "peer_pongs": peer_pongs,
                            });
                            let bytes = serde_json::to_vec(&payload)?;
                            let pong = Event { event_type: "pong".to_string(), data: bytes };
                            match agent_writer.publish_agent_event(pong) {
                                Ok(_) => info!("[Agent {}] Sent pong (including {} peer pongs)", agent_id, peer_pongs.len()),
                                Err(e) => error!("[Agent {}] ERROR sending pong: {:?}", agent_id, e),
                            }
                        }
                    }
                    Some(Err(e)) => error!("[Agent {}] ERROR reading SyncerEvent sample: {:?}", agent_id, e),
                    _ => debug!("[Agent {}] No more SyncerEvent samples (stream ended)", agent_id),
                }
            }

            agent_res = agent_stream.next() => {
                match agent_res {
                    Some(Ok(sample)) => {
                        debug!("[Agent {}] Received AgentEvent sample: {:?}", agent_id, sample);
                        let ev = sample.into_value();
                        debug!("[Agent {}] AgentEvent value: event_type={}", agent_id, ev.event_type);
                        if ev.event_type == "pong" {
                            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&ev.data) {
                                if let Some(id) = v.get("agent_id").and_then(|x| x.as_u64()) {
                                    if id as u32 != agent_id {
                                        peer_pongs.push(format!("agent:{}", id));
                                        info!("[Agent {}] Heard peer pong from {}", agent_id, id);
                                    }
                                }
                            }
                        }
                    }
                    Some(Err(e)) => error!("[Agent {}] ERROR reading AgentEvent sample: {:?}", agent_id, e),
                    _ => debug!("[Agent {}] No more AgentEvent samples (stream ended)", agent_id),
                }
            }

            // Writer status events
            status = writer_status_stream.next() => {
                match status {
                    Some(s) => debug!("[Agent {}][WriterStatus] {:?}", agent_id, s),
                    _ => debug!("[Agent {}][WriterStatus] stream ended", agent_id),
                }
            }
            // Reader events (lifecycle/status of the reader)
            reader_evt = agent_event_stream.next() => {
                match reader_evt {
                    Some(evt) => debug!("[Agent {}][ReaderEvent] {:?}", agent_id, evt),
                    _ => debug!("[Agent {}][ReaderEvent] stream ended", agent_id),
                }
            }
            // Syncer reader events (debug)
            syncer_evt = syncer_event_stream.next() => {
                match syncer_evt {
                    Some(evt) => debug!("[Agent {}][SyncerReaderEvent] {:?}", agent_id, evt),
                    _ => debug!("[Agent {}][SyncerReaderEvent] stream ended", agent_id),
                }
            }
        }
    }
}
