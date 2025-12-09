use rustdds::{DomainParticipant, QosPolicies, Topic, TopicKind};
use rustdds::policy::{Reliability, Durability};

pub fn create_common_qos() -> QosPolicies {
    QosPolicies::builder()
        .reliability(Reliability::Reliable { max_blocking_time: rustdds::Duration::from_millis(100) })
        .durability(Durability::TransientLocal)
        .build()
}

pub fn create_topic(
    participant: &DomainParticipant,
    name: &str,
    type_name: &str,
    qos: Option<&QosPolicies>,
    kind: TopicKind,
) -> Topic {
    let default_qos = create_common_qos();
    let qos_to_use = qos.unwrap_or(&default_qos);

    participant.create_topic(
        name.to_string(),
        type_name.to_string(),
        qos_to_use,
        kind,
    ).unwrap_or_else(|_| panic!("Failed to create topic {}", name))
}
