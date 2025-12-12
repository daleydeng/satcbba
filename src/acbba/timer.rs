use super::data_types::{NTPInfo, AgentId};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;
use rustdds::Timestamp;
use std::time;
use tokio::sync::mpsc::UnboundedSender;

pub async fn loop_time_client(
    _self_guid: AgentId,
    periodic_tx: UnboundedSender<bool>,
) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut rng = StdRng::seed_from_u64(seed);

    loop {
        let delta_time = time::Duration::from_secs(rng.random_range(15..40));
        tokio::time::sleep(delta_time).await;
        if let Err(e) = periodic_tx.send(true) {
            eprintln!("timer send error: {e}");
        }
    }
}

pub async fn loop_time_server(
    _self_guid: AgentId,
    periodic_tx: UnboundedSender<bool>,
    tx: UnboundedSender<NTPInfo>,
) {
    let seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let mut rng = StdRng::seed_from_u64(seed);

    loop {
        let delta_time = time::Duration::from_secs(rng.random_range(15..40));
        if let Err(e) = tx.send(NTPInfo::ServerResponse(Timestamp::now())) {
            eprintln!("timer ntp send error: {e}");
        }
        tokio::time::sleep(delta_time).await;
        if let Err(e) = periodic_tx.send(true) {
            eprintln!("timer send error: {e}");
        }
    }
}
