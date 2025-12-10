use futures::StreamExt;
use futures::future::join_all;
use serde_json::json;

use cbbadds::consensus::types::AgentId;
use cbbadds::dds::transport::{new_agent_transport, new_syncer_transport};
use cbbadds::dds::types::{AgentEvent, AgentStatus, Event};
use cbbadds::dds::utils::create_common_qos;

use std::time::{SystemTime, UNIX_EPOCH};
use tracing::info;
use tracing_subscriber;

async fn run_syncer() {
    info!("[Inproc Syncer] Starting...");
    let domain_id = 0u16;
    let qos = create_common_qos();
    let (syncer_writer, syncer_reader) =
        new_syncer_transport::<AgentStatus, cbbadds::sat::ExploreTask>(domain_id, qos);
    info!("[Inproc Syncer] Transport created");
    let mut agent_stream = syncer_reader.agent_event_stream;
    let mut ping_id: u64 = 0;

    loop {
        ping_id += 1;
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let payload = json!({"ping_id": ping_id, "ts": ts});
        let ping = Event {
            event_type: "ping".to_string(),
            data: serde_json::to_vec(&payload).unwrap(),
        };
        let _ = syncer_writer.publish_syncer_event(ping);
        info!("[Inproc Syncer] Sent ping {}", ping_id);

        let start = tokio::time::Instant::now();
        while tokio::time::Instant::now() - start < tokio::time::Duration::from_secs(3) {
            if let Some(Ok(sample)) = agent_stream.next().await {
                if let rustdds::with_key::Sample::Value(ev) = sample.into_value() {
                    if ev.event_type == "pong" {
                        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&ev.data) {
                            info!("[Inproc Syncer] Pong: {}", v);
                        }
                    }
                }
            }
        }
    }
}

async fn run_agent(agent_id: u32) {
    info!("[Inproc Agent {}] Starting...", agent_id);
    let domain_id = 0u16;
    let qos = create_common_qos();
    let (agent_writer, agent_reader) = new_agent_transport::<AgentStatus, cbbadds::sat::ExploreTask>(
        AgentId(agent_id),
        domain_id,
        qos,
    );
    info!("[Inproc Agent {}] Transport created", agent_id);

    let mut syncer_stream = agent_reader.syncer_event_stream;
    let mut agent_stream = agent_reader.peer_agent_event_stream;
    let mut peer_pongs: Vec<String> = Vec::new();

    loop {
        tokio::select! {
            syncer_res = syncer_stream.next() => {
                if let Some(Ok(sample)) = syncer_res {
                    let ev = sample.into_value();
                    if ev.event_type == "ping" {
                        // extract ping_id if present so we echo it back with the pong
                        let ping_id = serde_json::from_slice::<serde_json::Value>(&ev.data)
                            .ok()
                            .and_then(|v| v.get("ping_id").and_then(|x| x.as_u64()))
                            .unwrap_or(0);
                        let payload = json!({"agent_id": agent_id, "ping_id": ping_id, "peer_pongs": peer_pongs});
                        let bytes = serde_json::to_vec(&payload).unwrap();
                        let pong = AgentEvent { agent_id: AgentId(agent_id), event_type: "pong".to_string(), data: bytes };
                        let _ = agent_writer.publish_agent_event(pong);
                        info!("[Inproc Agent {}] Sent pong ({} peers) for ping {}", agent_id, peer_pongs.len(), ping_id);
                    }
                }
            }

            agent_res = agent_stream.next() => {
                if let Some(Ok(sample)) = agent_res {
                    if let rustdds::with_key::Sample::Value(ev) = sample.into_value() {
                        if ev.event_type == "pong" {
                            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&ev.data) {
                                if let Some(id) = v.get("agent_id").and_then(|x| x.as_u64()) {
                                        if id as u32 != agent_id {
                                            peer_pongs.push(format!("agent:{}", id));
                                            info!("[Inproc Agent {}] Heard peer pong from {}", agent_id, id);
                                        }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing subscriber so we can capture rustdds logs via RUST_LOG
    let _ = tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                tracing_subscriber::EnvFilter::new("info,rustdds=warn,rustdds::rtps=warn")
            }),
        )
        .try_init();
    info!("Starting in-process runner: 1 syncer + 3 agents");

    let mut tasks = Vec::new();
    // Spawn agents first so their readers are ready
    for i in 1..=3u32 {
        tasks.push(tokio::spawn(run_agent(i)));
    }
    // small delay to allow agents to create readers
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    tasks.push(tokio::spawn(run_syncer()));

    let _ = join_all(tasks).await;
    Ok(())
}
