use super::data_types::{WinnersMessage, NTPInfo, UpdateInfo, AgentId};
use rustdds::{
    Timestamp,
    dds::no_key::DataWriter,
    serialization::CDRSerializerAdapter,
};
use tokio::sync::mpsc::UnboundedReceiver;

pub async fn loop_send(
    _self_guid: AgentId,
    task_writer: DataWriter<UpdateInfo, CDRSerializerAdapter<UpdateInfo>>,
    mut un_to_s_rx: UnboundedReceiver<UpdateInfo>,

    async_writer: DataWriter<WinnersMessage, CDRSerializerAdapter<WinnersMessage>>,
    mut bh_to_s_rx: UnboundedReceiver<WinnersMessage>,

    ntp_writer: DataWriter<NTPInfo, CDRSerializerAdapter<NTPInfo>>,
    mut t_to_s_rx: UnboundedReceiver<NTPInfo>,

) {
    loop {
        tokio::select! {
            Some(data) = un_to_s_rx.recv() => {
                if let Err(e) = task_writer.write(data, None) {
                    eprintln!("sender: task_updation_writer error: {e}");
                }
            }
            Some(data) = bh_to_s_rx.recv() => {
                if let Err(e) = async_writer.write(data, None) {
                    eprintln!("sender: async_writer error: {e}");
                }
            }
            Some(data) = t_to_s_rx.recv() => {
                if let Err(e) = ntp_writer.write(data, Some(Timestamp::now())) {
                    eprintln!("sender: ntp_writer error: {e}");
                }
            }
            else => break,
        }
    }
}
