use clap::Parser;
use futures::StreamExt;
use rustdds::StatusEvented;
use satcbba::dds::transport::new_syncer_transport;
use satcbba::dds::types::Event;
use satcbba::dds::utils::create_common_qos;
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::time::Duration;
use tracing::{debug, error, info};
use tracing_subscriber;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Ping interval in seconds
    #[arg(long, default_value_t = 3)]
    interval: u64,

    /// DDS domain id to use
    #[arg(long, default_value_t = 0)]
    domain: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    let domain_id = args.domain;
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
    info!(
        "[Syncer] Starting Syncer (ex3 ping/pong) on domain {} with interval {}s",
        domain_id, args.interval
    );
    let qos = create_common_qos();

    // Use transport helper to get writers/readers
    debug!("[Syncer] Creating transport...");
    let (syncer_writer, syncer_reader) = new_syncer_transport::<
        satcbba::dds::types::AgentStatus,
        satcbba::sat::ExploreTask,
    >(domain_id, qos);
    info!("[Syncer] Transport created. Listening for AgentEvents...");

    // Give DDS discovery a moment to match writers/readers before first ping
    tokio::time::sleep(Duration::from_secs(2)).await;

    // Writer status stream for debugging
    let mut writer_status_stream = syncer_writer.syncer_event_writer.as_async_status_stream();

    let mut agent_stream = syncer_reader.agent_event_stream;
    // Reader event stream for agent reader (lifecycle/status events)
    let mut agent_event_stream = agent_stream.async_event_stream();

    let mut ping_id: u64 = 0;

    loop {
        // publish ping
        ping_id += 1;
        let ts = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
        let payload = json!({"ping_id": ping_id, "ts": ts});
        let ping = Event {
            event_type: "ping".to_string(),
            data: serde_json::to_vec(&payload)?,
        };
        match syncer_writer.publish_syncer_event(ping) {
            Ok(_) => info!("[Syncer] Sent ping {}", ping_id),
            Err(e) => error!("[Syncer] ERROR sending ping {}: {:?}", ping_id, e),
        }

        // collect pongs for the specified interval
        let deadline =
            tokio::time::Instant::now() + tokio::time::Duration::from_secs(args.interval);
        while tokio::time::Instant::now() < deadline {
            tokio::select! {
                agent_res = agent_stream.next() => {
                    match agent_res {
                        Some(Ok(sample)) => {
                            debug!("[Syncer] Received sample: {:?}", sample);
                            let ev = sample.into_value();
                            debug!("[Syncer] Sample value: event_type={}", ev.event_type);
                            if ev.event_type == "pong" {
                                match serde_json::from_slice::<serde_json::Value>(&ev.data) {
                                    Ok(v) => info!("[Syncer] Pong data: {}", v),
                                    Err(e) => error!("[Syncer] ERROR decoding pong data: {:?}", e),
                                }
                            }
                        }
                        Some(Err(e)) => error!("[Syncer] ERROR reading sample: {:?}", e),
                        _ => debug!("[Syncer] No more samples (stream ended)"),
                    }
                }
                status = writer_status_stream.next() => {
                    match status {
                        Some(s) => debug!("[Syncer][WriterStatus] {:?}", s),
                        _ => debug!("[Syncer][WriterStatus] stream ended"),
                    }
                }
                // Reader lifecycle/status events
                reader_evt = agent_event_stream.next() => {
                    match reader_evt {
                        Some(evt) => debug!("[Syncer][ReaderEvent] {:?}", evt),
                        _ => debug!("[Syncer][ReaderEvent] stream ended"),
                    }
                }
                _ = tokio::time::sleep(Duration::from_millis(50)) => {}
            }
        }
    }

    // Ok(())
}
