use crate::error::Error;
use crate::consensus::types::{AgentId, Task, ConsensusMessage};
use crate::dds::types::{
    AgentStatus, AgentPhase,
    RequestFromManager, ReplyToManager, RequestFromSyncer, ReplyToSyncer,
    LogMessage,
    REQUEST_FROM_MANAGER_TOPIC, REPLY_TO_MANAGER_TOPIC, REQUEST_FROM_SYNCER_TOPIC, REPLY_TO_SYNCER_TOPIC,
    AGENT_STATE_TOPIC, AGENT_STATUS_TOPIC, AGENT_CONSENSUS_TOPIC, LOG_TOPIC
};
use crate::dds::utils::create_topic;
use crate::consensus::cbba::{CBBA, ScoreFunction};
use crate::consensus::types::Agent;
use rustdds::{DomainParticipant, QosPolicies, TopicKind, Keyed};
use rustdds::no_key::{DataWriter, DataReader, DataReaderStream};
use rustdds::with_key::{DataWriter as KeyedDataWriter, DataReader as KeyedDataReader, DataReaderStream as KeyedDataReaderStream};
use rustdds::serialization::{CDRSerializerAdapter, CDRDeserializerAdapter};
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
    state_writer: KeyedDdsDataWriter<S>,
    status_writer: KeyedDdsDataWriter<AgentStatus>,
    consensus_writer: KeyedDdsDataWriter<ConsensusMessage>,
    reply_to_syncer_writer: KeyedDdsDataWriter<ReplyToSyncer>,
    log_writer: DdsDataWriter<LogMessage>,
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
        self.status_writer.write(msg, None)
            .map_err(|e| Error::NetworkError(format!("DDS Status Write Error: {:?}", e)))
    }

    pub fn publish_state(&self, state: S) -> Result<(), Error> {
        self.state_writer.write(state, None)
            .map_err(|e| Error::NetworkError(format!("DDS State Write Error: {:?}", e)))
    }

    pub fn publish_reply_to_syncer(&self, reply: ReplyToSyncer) -> Result<(), Error> {
        self.reply_to_syncer_writer.write(reply, None)
            .map_err(|e| Error::NetworkError(format!("DDS ReplyToSyncer Write Error: {:?}", e)))
    }

    pub fn publish_log(&self, log: LogMessage) -> Result<(), Error> {
        self.log_writer.write(log, None)
            .map_err(|e| Error::NetworkError(format!("DDS Log Write Error: {:?}", e)))
    }

    pub fn publish_consensus_message(&self, message: ConsensusMessage) -> Result<(), Error> {
        self.consensus_writer.write(message, None)
            .map_err(|e| Error::NetworkError(format!("DDS Write Error: {:?}", e)))
    }
}

/// DDS-based network handler implementation for Agents (Reader part)
pub struct AgentReader<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    pub syncer_request_stream: DataReaderStream<RequestFromSyncer<T>>,
    pub peer_consensus_stream: KeyedDdsDataReaderStream<ConsensusMessage>,
}

/// DDS-based network handler implementation for Agents
pub fn new_agent_transport<S, T>(
    agent_id: AgentId,
    domain_id: u16,
    qos: QosPolicies,
) -> (AgentWriter<S>, AgentReader<T>)
where
    S: Keyed + Serialize + DeserializeOwned + std::fmt::Debug + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize + Into<u32>,
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    let participant = DomainParticipant::new(domain_id).expect("Create participant failed");

    let publisher = participant.create_publisher(&qos).expect("Create publisher failed");
    let subscriber = participant.create_subscriber(&qos).expect("Create subscriber failed");

    let syncer_request_topic = create_topic(&participant, REQUEST_FROM_SYNCER_TOPIC, "RequestFromSyncer", Some(&qos), TopicKind::NoKey);
    let syncer_reply_topic = create_topic(&participant, REPLY_TO_SYNCER_TOPIC, "ReplyToSyncer", Some(&qos), TopicKind::WithKey);
    let agent_state_topic = create_topic(&participant, AGENT_STATE_TOPIC, "AgentState", Some(&qos), TopicKind::WithKey);
    let agent_status_topic = create_topic(&participant, AGENT_STATUS_TOPIC, "AgentStatus", Some(&qos), TopicKind::WithKey);
    let agent_consensus_topic = create_topic(&participant, AGENT_CONSENSUS_TOPIC, "AgentConsensus", Some(&qos), TopicKind::WithKey);

    let log_topic = create_topic(&participant, LOG_TOPIC, "LogMessage", Some(&qos), TopicKind::NoKey);

    let state_writer = publisher.create_datawriter::<S, CDRSerializerAdapter<S>>(&agent_state_topic, None)
            .expect("Create state writer failed");
    let status_writer = publisher.create_datawriter::<AgentStatus, CDRSerializerAdapter<AgentStatus>>(&agent_status_topic, None)
            .expect("Create status writer failed");
    let consensus_writer = publisher.create_datawriter::<ConsensusMessage, CDRSerializerAdapter<ConsensusMessage>>(&agent_consensus_topic, None)
            .expect("Create consensus writer failed");
    let reply_to_syncer_writer = publisher.create_datawriter::<ReplyToSyncer, CDRSerializerAdapter<ReplyToSyncer>>(&syncer_reply_topic, None)
            .expect("Create reply to syncer writer failed");

    let peer_consensus_reader = subscriber.create_datareader::<ConsensusMessage, CDRDeserializerAdapter<ConsensusMessage>>(&agent_consensus_topic, None)
            .expect("Create peer consensus reader failed");
    let log_writer = publisher.create_datawriter_no_key::<LogMessage, CDRSerializerAdapter<LogMessage>>(&log_topic, None)
            .expect("Create log writer failed");

    let syncer_request_reader = subscriber.create_datareader_no_key::<RequestFromSyncer<T>, CDRDeserializerAdapter<RequestFromSyncer<T>>>(&syncer_request_topic, None)
            .expect("Create syncer request reader failed");

    let syncer_request_stream = syncer_request_reader.async_sample_stream();
    let peer_consensus_stream = peer_consensus_reader.async_sample_stream();

    let writer = AgentWriter {
        state_writer,
        status_writer,
        consensus_writer,
        reply_to_syncer_writer,
        log_writer,
        agent_id,
        _participant: participant,
    };

    let reader = AgentReader {
        syncer_request_stream,
        peer_consensus_stream,
    };

    (writer, reader)
}

/// DDS-based network handler implementation for Syncer (Writer part)
pub struct SyncerWriter<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    syncer_request_writer: DdsDataWriter<RequestFromSyncer<T>>,
    reply_to_manager_writer: DdsDataWriter<ReplyToManager>,
    log_writer: DdsDataWriter<LogMessage>,
    _participant: DomainParticipant,
}

impl<T> SyncerWriter<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    pub fn publish_syncer_request(&self, request: RequestFromSyncer<T>) -> Result<(), Error> {
        self.syncer_request_writer.write(request, None)
            .map_err(|e| Error::NetworkError(format!("DDS Syncer Write Error: {:?}", e)))
    }

    pub fn publish_reply_to_manager(&self, reply: ReplyToManager) -> Result<(), Error> {
        self.reply_to_manager_writer.write(reply, None)
            .map_err(|e| Error::NetworkError(format!("DDS ReplyToManager Write Error: {:?}", e)))
    }

    pub fn publish_log(&self, log: LogMessage) -> Result<(), Error> {
        self.log_writer.write(log, None)
            .map_err(|e| Error::NetworkError(format!("DDS Log Write Error: {:?}", e)))
    }
}

/// DDS-based network handler implementation for Syncer (Reader part)
pub struct SyncerReader<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    pub manager_request_stream: DataReaderStream<RequestFromManager<T>>,
    pub agent_status_stream: KeyedDdsDataReaderStream<AgentStatus>,
    pub reply_to_syncer_stream: KeyedDdsDataReaderStream<ReplyToSyncer>,
}

/// DDS-based network handler implementation for Syncer
pub fn new_syncer_transport<T>(domain_id: u16, qos: QosPolicies) -> (SyncerWriter<T>, SyncerReader<T>)
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    let participant = DomainParticipant::new(domain_id).expect("Create participant failed");

    let publisher = participant.create_publisher(&qos).expect("Create publisher failed");
    let subscriber = participant.create_subscriber(&qos).expect("Create subscriber failed");

    let syncer_topic = create_topic(&participant, REQUEST_FROM_SYNCER_TOPIC, "RequestFromSyncer", Some(&qos), TopicKind::NoKey);
    let reply_topic = create_topic(&participant, REPLY_TO_MANAGER_TOPIC, "ReplyToManager", Some(&qos), TopicKind::NoKey);
    let agent_status_topic = create_topic(&participant, AGENT_STATUS_TOPIC, "AgentStatus", Some(&qos), TopicKind::WithKey);
    let manager_topic = create_topic(&participant, REQUEST_FROM_MANAGER_TOPIC, "RequestFromManager", Some(&qos), TopicKind::NoKey);
    let log_topic = create_topic(&participant, LOG_TOPIC, "LogMessage", Some(&qos), TopicKind::NoKey);
    let syncer_reply_topic = create_topic(&participant, REPLY_TO_SYNCER_TOPIC, "ReplyToSyncer", Some(&qos), TopicKind::WithKey);

    let syncer_request_writer = publisher.create_datawriter_no_key::<RequestFromSyncer<T>, CDRSerializerAdapter<RequestFromSyncer<T>>>(&syncer_topic, None)
        .expect("Create syncer writer failed");

    let reply_to_manager_writer = publisher.create_datawriter_no_key::<ReplyToManager, CDRSerializerAdapter<ReplyToManager>>(&reply_topic, None)
        .expect("Create reply writer failed");

    let log_writer = publisher.create_datawriter_no_key::<LogMessage, CDRSerializerAdapter<LogMessage>>(&log_topic, None)
        .expect("Create log writer failed");

    let agent_status_reader = subscriber.create_datareader::<AgentStatus, CDRDeserializerAdapter<AgentStatus>>(&agent_status_topic, None)
        .expect("Create agent status reader failed");

    let manager_reader = subscriber.create_datareader_no_key::<RequestFromManager<T>, CDRDeserializerAdapter<RequestFromManager<T>>>(&manager_topic, None)
        .expect("Create manager reader failed");

    let reply_to_syncer_reader = subscriber.create_datareader::<ReplyToSyncer, CDRDeserializerAdapter<ReplyToSyncer>>(&syncer_reply_topic, None)
        .expect("Create reply to syncer reader failed");

    let writer = SyncerWriter {
        syncer_request_writer,
        reply_to_manager_writer,
        log_writer,
        _participant: participant,
    };

    let manager_request_stream = manager_reader.async_sample_stream();
    let agent_status_stream = agent_status_reader.async_sample_stream();
    let reply_to_syncer_stream = reply_to_syncer_reader.async_sample_stream();

    let reader = SyncerReader {
        manager_request_stream,
        agent_status_stream,
        reply_to_syncer_stream,
    };

    (writer, reader)
}

/// DDS-based network handler implementation for Manager (Writer part)
pub struct ManagerWriter<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    command_writer: DdsDataWriter<RequestFromManager<T>>,
    log_writer: DdsDataWriter<LogMessage>,
    _participant: DomainParticipant,
}

impl<T> ManagerWriter<T>
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
{
    pub fn publish_command(&self, command: RequestFromManager<T>) -> Result<(), Error> {
        self.command_writer.write(command, None)
            .map_err(|e| Error::NetworkError(format!("DDS Manager Command Write Error: {:?}", e)))
    }

    pub fn publish_log(&self, log: LogMessage) -> Result<(), Error> {
        self.log_writer.write(log, None)
            .map_err(|e| Error::NetworkError(format!("DDS Log Write Error: {:?}", e)))
    }
}

/// DDS-based network handler implementation for Manager (Reader part)
pub struct ManagerReader<S>
where
    S: Keyed + Serialize + DeserializeOwned + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize,
{
    pub agent_status_stream: KeyedDdsDataReaderStream<AgentStatus>,
    pub agent_state_stream: KeyedDdsDataReaderStream<S>,
    pub reply_to_manager_stream: DdsDataReaderStream<ReplyToManager>,
    pub log_stream: DdsDataReaderStream<LogMessage>,
}

/// DDS-based network handler implementation for Manager
pub fn new_manager_transport<T, S>(domain_id: u16, qos: QosPolicies) -> (ManagerWriter<T>, ManagerReader<S>)
where
    T: Task + Serialize + DeserializeOwned + Send + 'static,
    S: Keyed + Serialize + DeserializeOwned + Send + 'static,
    <S as Keyed>::K: for<'de> serde::Deserialize<'de> + Serialize,
{
    let participant = DomainParticipant::new(domain_id).expect("Create participant failed");

    let publisher = participant.create_publisher(&qos).expect("Create publisher failed");
    let subscriber = participant.create_subscriber(&qos).expect("Create subscriber failed");

    let manager_topic = create_topic(&participant, REQUEST_FROM_MANAGER_TOPIC, "RequestFromManager", Some(&qos), TopicKind::NoKey);
    let agent_status_topic = create_topic(&participant, AGENT_STATUS_TOPIC, "AgentStatus", Some(&qos), TopicKind::WithKey);
    let agent_state_topic = create_topic(&participant, AGENT_STATE_TOPIC, "AgentState", Some(&qos), TopicKind::WithKey);
    let reply_topic = create_topic(&participant, REPLY_TO_MANAGER_TOPIC, "ReplyToManager", Some(&qos), TopicKind::NoKey);
    let log_topic = create_topic(&participant, LOG_TOPIC, "LogMessage", Some(&qos), TopicKind::NoKey);

    let command_writer = publisher.create_datawriter_no_key::<RequestFromManager<T>, CDRSerializerAdapter<RequestFromManager<T>>>(&manager_topic, None)
        .expect("Create command writer failed");
    let log_writer = publisher.create_datawriter_no_key::<LogMessage, CDRSerializerAdapter<LogMessage>>(&log_topic, None)
        .expect("Create log writer failed");

    let agent_status_reader = subscriber.create_datareader::<AgentStatus, CDRDeserializerAdapter<AgentStatus>>(&agent_status_topic, None)
        .expect("Create agent status reader failed");
    let agent_state_reader = subscriber.create_datareader::<S, CDRDeserializerAdapter<S>>(&agent_state_topic, None)
        .expect("Create agent state reader failed");
    let reply_to_manager_reader = subscriber.create_datareader_no_key::<ReplyToManager, CDRDeserializerAdapter<ReplyToManager>>(&reply_topic, None)
        .expect("Create reply reader failed");
    let log_reader = subscriber.create_datareader_no_key::<LogMessage, CDRDeserializerAdapter<LogMessage>>(&log_topic, None)
        .expect("Create log reader failed");

    let writer = ManagerWriter {
        command_writer,
        log_writer,
        _participant: participant,
    };

    let agent_status_stream = agent_status_reader.async_sample_stream();
    let agent_state_stream = agent_state_reader.async_sample_stream();
    let reply_to_manager_stream = reply_to_manager_reader.async_sample_stream();
    let log_stream = log_reader.async_sample_stream();

    let reader = ManagerReader {
        agent_status_stream,
        agent_state_stream,
        reply_to_manager_stream,
        log_stream,
    };

    (writer, reader)
}
