use super::data_types::{WinnersMessage, NTPInfo, UpdateInfo, AgentId};
use futures::stream::StreamExt;
use parking_lot::RwLock;
use rustdds::{
    Duration, Timestamp,
    dds::no_key::DataReader,
    serialization::CDRDeserializerAdapter,
};
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedSender;

pub async fn loop_receiver_messages(
    self_guid: AgentId,
    task_reader: DataReader<Vec<UpdateInfo>, CDRDeserializerAdapter<Vec<UpdateInfo>>>,
    l_to_e_tx: UnboundedSender<Vec<UpdateInfo>>,
    async_reader: DataReader<WinnersMessage, CDRDeserializerAdapter<WinnersMessage>>,
    l_to_h_tx: UnboundedSender<WinnersMessage>,
) {
    let mut task_stream = task_reader.async_sample_stream();
    let mut async_stream = async_reader.async_sample_stream();

    loop {
        tokio::select! {
            Some(result) = task_stream.next() => {
                if let Ok(sample) = result {
                    let info = sample.sample_info();
                    let pub_guid = info.publication_handle();
                    if pub_guid != self_guid.0 {
                        let data = sample.into_value();
                        if let Err(e) = l_to_e_tx.send(data) {
                            eprintln!("receiver: task send error: {e}");
                        }
                    }
                }
            }
            Some(result) = async_stream.next() => {
                if let Ok(sample) = result {
                    let info = sample.sample_info();
                    let pub_guid = info.publication_handle();
                    if pub_guid != self_guid.0 {
                        let data = sample.into_value();
                        if let Err(e) = l_to_h_tx.send(data) {
                            eprintln!("receiver: bundle send error: {e}");
                        }
                    }
                }
            }
            else => break,
        }
    }
}

pub async fn loop_receiver_ntp_client(
    _self_guid: AgentId,
    ntp_reader: DataReader<NTPInfo, CDRDeserializerAdapter<NTPInfo>>,
    lock_time_offset: Arc<RwLock<Option<Duration>>>,
) {
    let mut ntp_stream = ntp_reader.async_sample_stream();

    while let Some(result) = ntp_stream.next().await {
        if let Ok(sample) = result {
            let ntp_instance = sample.into_value();
            if let NTPInfo::ServerResponse(source_timestamp) = ntp_instance {
                let mut w = lock_time_offset.write();
                *w = Some(Timestamp::now() - source_timestamp);
            }
        }
    }
}

pub async fn loop_receiver_ntp_server(
    self_guid: AgentId,
    ntp_reader: DataReader<NTPInfo, CDRDeserializerAdapter<NTPInfo>>,
) {
    let mut ntp_stream = ntp_reader.async_sample_stream();

    while let Some(result) = ntp_stream.next().await {
        if let Ok(sample) = result {
            let pub_guid = sample.sample_info().publication_handle();
            let ntp_instance = sample.into_value();
            if let NTPInfo::ServerResponse(_) = ntp_instance {
                if pub_guid != self_guid.0 {
                    panic!("Error! Multiple NTP Servers Detected.");
                }
            }
        }
    }
}
