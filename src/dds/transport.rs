use crate::consensus::cbba::{CBBA, ScoreFunction};
use crate::consensus::types::Agent;
use crate::consensus::types::{AgentId, ConsensusMessage, Task};
use crate::dds::types::{
    AGENT_COMMAND_TOPIC, AGENT_CONSENSUS_TOPIC, AGENT_EVENT_TOPIC, AGENT_REPLY_TOPIC,
    AGENT_STATE_TOPIC, AGENT_STATUS_TOPIC, AgentCommand, AgentPhase, AgentReply, AgentStatus,
    Event, LOG_TOPIC, LogMessage, SYNCER_EVENT_TOPIC,
};
use crate::dds::utils::create_topic;
use crate::error::Error;
use rustdds::no_key::{DataReader, DataReaderStream, DataWriter};
use rustdds::serialization::{CDRDeserializerAdapter, CDRSerializerAdapter};
use rustdds::with_key::{
    DataReader as KeyedDataReader, DataReaderStream as KeyedDataReaderStream,
    DataWriter as KeyedDataWriter,
};
use rustdds::{DomainParticipant, Keyed, QosPolicies, TopicKind};
use serde::{Serialize, de::DeserializeOwned};

pub type DdsDataWriter<T> = DataWriter<T, CDRSerializerAdapter<T>>;
pub type DdsDataReader<T> = DataReader<T, CDRDeserializerAdapter<T>>;
pub type DdsDataReaderStream<T> = DataReaderStream<T, CDRDeserializerAdapter<T>>;
pub type KeyedDdsDataWriter<T> = KeyedDataWriter<T, CDRSerializerAdapter<T>>;
pub type KeyedDdsDataReader<T> = KeyedDataReader<T, CDRDeserializerAdapter<T>>;
pub type KeyedDdsDataReaderStream<T> = KeyedDataReaderStream<T, CDRDeserializerAdapter<T>>;

// Implement Keyed for CBBA State
impl<T: Task, A: Agent, C: ScoreFunction<T, A>> Keyed for CBBA<T, A, C> {
    type K = u32;
    fn key(&self) -> Self::K {
        self.agent.id().0
    }
}

/// DDS-based network handler implementation for Agents (Writer part)
pub struct AgentWriter<S>
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize,
{
    pub state_writer: KeyedDdsDataWriter<S>,
    pub status_writer: KeyedDdsDataWriter<AgentStatus>,
    pub consensus_writer: KeyedDdsDataWriter<ConsensusMessage>,
    pub reply_to_syncer_writer: KeyedDdsDataWriter<AgentReply>,
    pub agent_event_writer: DdsDataWriter<Event>,
    pub log_writer: DdsDataWriter<LogMessage>,
    agent_id: AgentId,
    _participant: DomainParticipant,
}

impl<S> AgentWriter<S>
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize + Into<u32>,
{
    pub fn publish_status(&self, phase: AgentPhase) -> Result<(), Error> {
        let msg = AgentStatus {
            agent_id: self.agent_id,
            phase,
        };
        self.status_writer
            .write(msg, None)
            .map_err(|e| Error::NetworkError(format!("DDS Status Write Error: {:?}", e)))
    }

    pub fn publish_state(&self, state: S) -> Result<(), Error> {
        self.state_writer
            .write(state, None)
            .map_err(|e| Error::NetworkError(format!("DDS State Write Error: {:?}", e)))
    }

    pub fn publish_reply_to_syncer(&self, reply: AgentReply) -> Result<(), Error> {
        self.reply_to_syncer_writer
            .write(reply, None)
            .map_err(|e| Error::NetworkError(format!("DDS AgentReply Write Error: {:?}", e)))
    }

    pub fn publish_log(&self, log: LogMessage) -> Result<(), Error> {
        self.log_writer
            .write(log, None)
            .map_err(|e| Error::NetworkError(format!("DDS Log Write Error: {:?}", e)))
    }

    pub fn publish_consensus_message(&self, message: ConsensusMessage) -> Result<(), Error> {
        self.consensus_writer
            .write(message, None)
            .map_err(|e| Error::NetworkError(format!("DDS Write Error: {:?}", e)))
    }

    pub fn publish_agent_event(&self, event: Event) -> Result<(), Error> {
        self.agent_event_writer
            .write(event, None)
            .map_err(|e| Error::NetworkError(format!("DDS AgentEvent Write Error: {:?}", e)))
    }
}

/// DDS-based network handler implementation for Agents (Reader part)
pub struct AgentReader<S, T>
where
    S: Serialize + DeserializeOwned + Send + 'static,
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    pub syncer_request_stream: DataReaderStream<AgentCommand<T, ConsensusMessage, S>>,
    pub peer_consensus_stream: KeyedDdsDataReaderStream<ConsensusMessage>,
    pub syncer_event_stream: DdsDataReaderStream<Event>,
    pub peer_agent_event_stream: DdsDataReaderStream<Event>,
}

pub fn new_agent_transport<S, T>(
    agent_id: AgentId,
    domain_id: u16,
    qos: QosPolicies,
) -> (AgentWriter<S>, AgentReader<S, T>)
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize + Into<u32>,
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    let participant = DomainParticipant::new(domain_id).expect("Create participant failed");

    let publisher = participant
        .create_publisher(&qos)
        .expect("Create publisher failed");
    let subscriber = participant
        .create_subscriber(&qos)
        .expect("Create subscriber failed");

    // Standard topics
    let syncer_request_topic = create_topic(
        &participant,
        AGENT_COMMAND_TOPIC,
        "AgentCommand",
        Some(&qos),
        TopicKind::NoKey,
    );
    let syncer_reply_topic = create_topic(
        &participant,
        AGENT_REPLY_TOPIC,
        "AgentReply",
        Some(&qos),
        TopicKind::WithKey,
    );
    let agent_state_topic = create_topic(
        &participant,
        AGENT_STATE_TOPIC,
        "AgentState",
        Some(&qos),
        TopicKind::WithKey,
    );
    let agent_status_topic = create_topic(
        &participant,
        AGENT_STATUS_TOPIC,
        "AgentStatus",
        Some(&qos),
        TopicKind::WithKey,
    );
    let agent_consensus_topic = create_topic(
        &participant,
        AGENT_CONSENSUS_TOPIC,
        "AgentConsensus",
        Some(&qos),
        TopicKind::WithKey,
    );

    // Event topics for ex3
    let syncer_event_topic = create_topic(
        &participant,
        SYNCER_EVENT_TOPIC,
        "Event",
        Some(&qos),
        TopicKind::NoKey,
    );

    let agent_event_topic = create_topic(
        &participant,
        AGENT_EVENT_TOPIC,
        "Event",
        Some(&qos),
        TopicKind::NoKey,
    );

    let log_topic = create_topic(
        &participant,
        LOG_TOPIC,
        "LogMessage",
        Some(&qos),
        TopicKind::NoKey,
    );

    // Writers
    let state_writer = publisher
        .create_datawriter(&agent_state_topic, None)
        .expect("Create state writer failed");
    let status_writer = publisher
        .create_datawriter(&agent_status_topic, None)
        .expect("Create status writer failed");
    let consensus_writer = publisher
        .create_datawriter(&agent_consensus_topic, None)
        .expect("Create consensus writer failed");
    let reply_to_syncer_writer = publisher
        .create_datawriter(&syncer_reply_topic, None)
        .expect("Create reply to syncer writer failed");

    let agent_event_writer = publisher
        .create_datawriter_no_key(&agent_event_topic, None)
        .expect("Create agent event writer failed");

    let log_writer = publisher
        .create_datawriter_no_key(&log_topic, None)
        .expect("Create log writer failed");

    // Readers
    let peer_consensus_reader = subscriber
        .create_datareader(&agent_consensus_topic, None)
        .expect("Create peer consensus reader failed");

    let syncer_request_reader = subscriber
        .create_datareader_no_key(&syncer_request_topic, None)
        .expect("Create syncer request reader failed");

    let syncer_event_reader = subscriber
        .create_datareader_no_key(&syncer_event_topic, None)
        .expect("Create syncer event reader failed");

    let peer_agent_event_reader = subscriber
        .create_datareader_no_key(&agent_event_topic, None)
        .expect("Create peer agent event reader failed");

    // Async streams
    let syncer_request_stream = syncer_request_reader.async_sample_stream();
    let peer_consensus_stream = peer_consensus_reader.async_sample_stream();
    let syncer_event_stream = syncer_event_reader.async_sample_stream();
    let peer_agent_event_stream = peer_agent_event_reader.async_sample_stream();

    let writer = AgentWriter {
        state_writer,
        status_writer,
        consensus_writer,
        reply_to_syncer_writer,
        agent_event_writer,
        log_writer,
        agent_id,
        _participant: participant,
    };

    let reader = AgentReader {
        syncer_request_stream,
        peer_consensus_stream,
        syncer_event_stream,
        peer_agent_event_stream,
    };

    (writer, reader)
}

/// DDS-based network handler implementation for Syncer (Writer part)
pub struct SyncerWriter<T, S>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
    S: Serialize + DeserializeOwned + Send + std::fmt::Debug + 'static,
{
    pub syncer_request_writer: DdsDataWriter<AgentCommand<T, ConsensusMessage, S>>,
    pub syncer_event_writer: DdsDataWriter<Event>,
    pub log_writer: DdsDataWriter<LogMessage>,
    _participant: DomainParticipant,
}

impl<T, S> SyncerWriter<T, S>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
    S: Serialize + DeserializeOwned + Send + std::fmt::Debug + 'static,
{
    pub fn publish_syncer_request(
        &self,
        request: AgentCommand<T, ConsensusMessage, S>,
    ) -> Result<(), Error> {
        self.syncer_request_writer
            .write(request, None)
            .map_err(|e| Error::NetworkError(format!("DDS Syncer Write Error: {:?}", e)))
    }

    pub fn publish_syncer_event(&self, ev: Event) -> Result<(), Error> {
        self.syncer_event_writer
            .write(ev, None)
            .map_err(|e| Error::NetworkError(format!("DDS Syncer Event Write Error: {:?}", e)))
    }

    pub fn publish_log(&self, log: LogMessage) -> Result<(), Error> {
        self.log_writer
            .write(log, None)
            .map_err(|e| Error::NetworkError(format!("DDS Log Write Error: {:?}", e)))
    }
}

/// DDS-based network handler implementation for Syncer (Reader part)
pub struct SyncerReader<S>
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize,
{
    pub agent_state_stream: KeyedDdsDataReaderStream<S>,
    pub agent_status_stream: KeyedDdsDataReaderStream<AgentStatus>,
    pub reply_to_syncer_stream: KeyedDdsDataReaderStream<AgentReply>,
    pub agent_event_stream: DdsDataReaderStream<Event>,
}

/// DDS-based network handler implementation for Syncer
pub fn new_syncer_transport<S, T>(
    domain_id: u16,
    qos: QosPolicies,
) -> (SyncerWriter<T, S>, SyncerReader<S>)
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize,
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    let participant = DomainParticipant::new(domain_id).expect("Create participant failed");

    let publisher = participant
        .create_publisher(&qos)
        .expect("Create publisher failed");
    let subscriber = participant
        .create_subscriber(&qos)
        .expect("Create subscriber failed");

    let syncer_topic = create_topic(
        &participant,
        AGENT_COMMAND_TOPIC,
        "AgentCommand",
        Some(&qos),
        TopicKind::NoKey,
    );
    let syncer_event_topic = create_topic(
        &participant,
        SYNCER_EVENT_TOPIC,
        "Event",
        Some(&qos),
        TopicKind::NoKey,
    );
    let agent_event_topic = create_topic(
        &participant,
        AGENT_EVENT_TOPIC,
        "Event",
        Some(&qos),
        TopicKind::NoKey,
    );
    // Manager reply topic removed; Syncer coordinates without Manager.
    let agent_status_topic = create_topic(
        &participant,
        AGENT_STATUS_TOPIC,
        "AgentStatus",
        Some(&qos),
        TopicKind::WithKey,
    );
    let agent_state_topic = create_topic(
        &participant,
        AGENT_STATE_TOPIC,
        "AgentState",
        Some(&qos),
        TopicKind::WithKey,
    );
    let log_topic = create_topic(
        &participant,
        LOG_TOPIC,
        "LogMessage",
        Some(&qos),
        TopicKind::NoKey,
    );
    let syncer_reply_topic = create_topic(
        &participant,
        AGENT_REPLY_TOPIC,
        "AgentReply",
        Some(&qos),
        TopicKind::WithKey,
    );

    let syncer_request_writer = publisher
        .create_datawriter_no_key(&syncer_topic, None)
        .expect("Create syncer writer failed");

    let syncer_event_writer = publisher
        .create_datawriter_no_key(&syncer_event_topic, None)
        .expect("Create syncer event writer failed");

    let log_writer = publisher
        .create_datawriter_no_key(&log_topic, None)
        .expect("Create log writer failed");

    let agent_status_reader = subscriber
        .create_datareader(&agent_status_topic, None)
        .expect("Create agent status reader failed");

    let agent_state_reader = subscriber
        .create_datareader(&agent_state_topic, None)
        .expect("Create agent state reader failed");

    let reply_to_syncer_reader = subscriber
        .create_datareader(&syncer_reply_topic, None)
        .expect("Create reply to syncer reader failed");

    let agent_event_reader = subscriber
        .create_datareader_no_key(&agent_event_topic, None)
        .expect("Create agent event reader failed");

    let writer = SyncerWriter {
        syncer_request_writer,
        syncer_event_writer,
        log_writer,
        _participant: participant,
    };

    let agent_status_stream = agent_status_reader.async_sample_stream();
    let agent_state_stream = agent_state_reader.async_sample_stream();
    let reply_to_syncer_stream = reply_to_syncer_reader.async_sample_stream();

    let agent_event_stream = agent_event_reader.async_sample_stream();

    let reader = SyncerReader {
        agent_state_stream,
        agent_status_stream,
        reply_to_syncer_stream,
        agent_event_stream,
    };

    (writer, reader)
}

// Manager transport removed: Syncer handles coordination without a separate Manager.
