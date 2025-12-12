use crate::dds::data_types::WinningMessage;

use super::{
    async_builder, evaluator, async_handler, receiver, sender, timer, utils,
    data_types::{WinnersMessage, NTPInfo, Player, TaskInfo, UpdateInfo, TaskId, AgentId},
};
use chashmap::CHashMap;
use parking_lot::RwLock;
use rustdds::{
    DomainParticipant, Duration, RTPSEntity, Timestamp, TopicKind,
    policy::{DestinationOrder, History, Lifespan, Ownership, Reliability},
    qos::{QosPolicies, QosPolicyBuilder},
    serialization::{CDRSerializerAdapter, CDRDeserializerAdapter}
};
use std::{collections::HashMap, sync::Arc};
use tokio::sync::mpsc;

// use local_ip_address::local_ip;

#[tokio::main(flavor = "multi_thread")]
pub async fn run_player(player_type: Player) {
    let participant = DomainParticipant::new(0).unwrap();
    let lock_time_offset: Arc<RwLock<Option<Duration>>> = Arc::new(RwLock::new(None));
    // let lock_is_server = Arc::new(RwLock::new(false));
    let self_guid = participant.guid();
    let self_agent = AgentId(self_guid);

    const QOS: QosPolicies = QosPolicyBuilder::new()
        // .durability(Durability::Volatile)
        // .deadline(Deadline(Duration::INFINITE))
        // .latency_budget(LatencyBudget {
        //     duration: Duration::ZERO,
        // })
        .ownership(Ownership::Shared)
        // .liveliness(Liveliness::Automatic {
        //     lease_duration: Duration::INFINITE,
        // })
        .reliability(Reliability::Reliable {
            max_blocking_time: Duration::ZERO,
        })
        .destination_order(DestinationOrder::ByReceptionTimestamp)
        .history(History::KeepLast { depth: 10 })
        .lifespan(Lifespan {
            duration: Duration::INFINITE,
        })
        .build();

    // Some DDS Topic that we can write and read from (basically only binds readers
    // and writers together)
    let task_topic = participant
        .create_topic(
            "task_topic".to_string(),
            "TaskUpdationType".to_string(),
            &QOS,
            TopicKind::NoKey,
        )
        .unwrap();

    let async_topic = participant
        .create_topic(
            "async_topic".to_string(),
            "Bundle".to_string(),
            &QOS,
            TopicKind::NoKey,
        )
        .unwrap();

    let ntp_topic = participant
        .create_topic(
            "ntp_topic".to_string(),
            "NTPInfo".to_string(),
            &QOS,
            TopicKind::NoKey,
        )
        .unwrap();

    let task_pub = participant.create_publisher(&QOS).unwrap();
    let async_pub = participant.create_publisher(&QOS).unwrap();
    let ntp_pub = participant.create_publisher(&QOS).unwrap();

    let task_writer = task_pub
        .create_datawriter_no_key::<UpdateInfo, CDRSerializerAdapter<UpdateInfo>>(
            &task_topic,
            Some(QOS),
        )
        .unwrap();
    let async_writer = async_pub
        .create_datawriter_no_key::<WinnersMessage, CDRSerializerAdapter<WinnersMessage>>(
            &async_topic,
            Some(QOS),
        )
        .unwrap();
    let ntp_writer = ntp_pub
        .create_datawriter_no_key_cdr(&ntp_topic,
            Some(QOS))
        .unwrap();

    let task_sub = participant.create_subscriber(&QOS).unwrap();
    let async_sub = participant.create_subscriber(&QOS).unwrap();
    let ntp_sub = participant.create_subscriber(&QOS).unwrap();

    let task_reader = task_sub
        .create_datareader_no_key::<Vec<UpdateInfo>, CDRDeserializerAdapter<Vec<UpdateInfo>>>(
            &task_topic,
            Some(QOS),
        )
        .unwrap();
    let async_reader = async_sub
        .create_datareader_no_key::<WinnersMessage, CDRDeserializerAdapter<WinnersMessage>>(
            &async_topic,
            Some(QOS),
        )
        .unwrap();
    let ntp_reader = ntp_sub
        .create_datareader_no_key_cdr::<NTPInfo>(&ntp_topic, Some(QOS))
        .unwrap();

    let (l_to_e_tx, l_to_e_rx) = mpsc::unbounded_channel();
    let (_un_to_s_tx, un_to_s_rx) = mpsc::unbounded_channel(); // TODO : 未来还需补充，谁来发送任务变化列表？ 向sender_task_updation_tx中写入？
    let (l_to_h_tx, l_to_h_rx) = mpsc::unbounded_channel();
    let (bh_to_s_tx, bh_to_s_rx) = mpsc::unbounded_channel();
    let (t_to_s_tx, t_to_s_rx) = mpsc::unbounded_channel();
    let (h_to_b_tx, h_to_b_rx) = mpsc::unbounded_channel::<Timestamp>();
    let (t_to_b_tx, t_to_b_rx) = mpsc::unbounded_channel::<bool>();
    let (e_to_b_tx, e_to_b_rx) = mpsc::unbounded_channel::<HashMap<TaskId, Option<TaskInfo>>>();

    let shared_cognition_core : Arc<CHashMap<TaskId, WinningMessage>> = Arc::new(CHashMap::with_capacity(100));

    let h_timer = tokio::spawn(async move {
        match player_type {
            Player::Client => {
                timer::loop_time_client(self_agent, t_to_b_tx).await;
            }
            Player::Server => {
                timer::loop_time_server(self_agent, t_to_b_tx, t_to_s_tx).await;
            }
        }
    });

    let h_sender = tokio::spawn(async move {
            sender::loop_send(
            self_agent,
            task_writer,
            un_to_s_rx,
            async_writer,
            bh_to_s_rx,
            ntp_writer,
            t_to_s_rx,
        ).await;
    });

    let h_receiver_messages = tokio::spawn(async move {
        receiver::loop_receiver_messages(
            self_agent,
            task_reader,
            l_to_e_tx,
            async_reader,
            l_to_h_tx,
        ).await;
    });

    let lock_time_offset_clone_ntp = lock_time_offset.clone();
    let h_receiver_ntp = tokio::spawn(async move {
        match player_type {
            Player::Client => {
                receiver::loop_receiver_ntp_client(self_agent, ntp_reader, lock_time_offset_clone_ntp).await;
            }
            Player::Server => {
                receiver::loop_receiver_ntp_server(self_agent, ntp_reader).await;
            }
        }
    });

    let e_to_b_tx_clone = e_to_b_tx.clone();
    let h_evaluator = tokio::spawn(async move {
        evaluator::loop_evaluate(
            self_agent,
            l_to_e_rx,
            e_to_b_tx_clone,
        ).await;
    });

    let h_init_task_pool = tokio::spawn(async move {
        println!("{:?} async init task pool", self_guid);

        // Simulate: need 更真实、符合实际的任务信息生成
        let mut task_pool: HashMap<TaskId, Option<TaskInfo>> = HashMap::with_capacity(100);
        for i in 0..100u32 {
            let task_id = TaskId(i);
            let task_info = utils::generate_random_task_info(task_id);
            task_pool.insert(task_id, Some(task_info));
        }
        e_to_b_tx
            .send(task_pool)
            .unwrap_or_else(|e| eprintln!("init task pool send error: {e}"));
    });

    let shared_cognition_core_clone = shared_cognition_core.clone();
    let bh_to_s_tx_clone = bh_to_s_tx.clone();
    let h_builder = tokio::spawn(async move {
        async_builder::loop_builder(
            self_agent,
            h_to_b_rx,
            t_to_b_rx,
            bh_to_s_tx_clone,
            e_to_b_rx,
            shared_cognition_core_clone,
        ).await;
    });

    // let lock_time_offset_clone_3 = lock_time_offset.clone();
    // let h_to_b_tx2 = h_to_b_tx.clone();
    // let handle_3 = tokio::spawn(async move {
    //     async_builder::AsyncBuilder::first_time_building(
    //         self_guid,
    //         player_type,
    //         h_to_b_tx2,
    //         lock_time_offset_clone_3,
    //     ).await;
    // });

    // let handle_4 = tokio::spawn(async move {
    //     async_handler::loop_handler(
    //         self_guid,
    //         player_type,
    //         l_to_h_rx,
    //         bh_to_s_tx,
    //         h_to_b_tx,
    //         lock_time_offset,
    //         shared_cognition_core_clone,
    //     ).await;
    // });

    // let _ = tokio::join!(h_timer, h_sender, h_receiver_messages, h_receiver_ntp, handle_1, handle_2, handle_3, handle_4);
    let _ = tokio::join!(h_timer, h_sender, h_receiver_messages, h_receiver_ntp, h_evaluator, h_init_task_pool, h_builder);
}
