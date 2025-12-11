use crate::consensus::types::{AddedTasks, AgentId, ReleasedTasks, Timestamp};
use rustdds::Keyed;
use serde::{Deserialize, Serialize};

// --- Topic Names ---
pub const AGENT_COMMAND_TOPIC: &str = "AgentCommand";
pub const AGENT_REPLY_TOPIC: &str = "AgentReply";
pub const AGENT_STATE_TOPIC: &str = "AgentState"; // Keyed
pub const AGENT_STATUS_TOPIC: &str = "AgentStatus"; // Keyed
pub const AGENT_CONSENSUS_TOPIC: &str = "AgentConsensus"; // Keyed
pub const LOG_TOPIC: &str = "SystemLog";
pub const AGENT_EVENT_TOPIC: &str = "AgentEvent";
pub const SYNCER_EVENT_TOPIC: &str = "SyncerEvent";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub timestamp: Timestamp,
    pub source: String,
    pub level: String, // "INFO", "WARN", "ERROR"
    pub content: String,
}

/// Generic binary Event wrapper used for arbitrary event payloads
/// Direction: can be used on AgentEvent or SyncerEvent topics
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Event {
    /// Human-readable event type identifier
    pub event_type: String,
    /// Opaque binary payload
    pub data: Vec<u8>,
}

/// Commands sent from Syncer to Agents
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum AgentCommand<T, M, S> {
    Initialization {
        request_id: String,
        state: Option<S>,
    },
    /// Optionally carries a set of tasks for bundle construction
    BundlingConstruction {
        request_id: String,
        tasks: Option<Vec<T>>,
    },
    /// Optionally carries consensus messages for conflict resolution
    ConflictResolution {
        request_id: String,
        messages: Option<Vec<M>>,
    },
    /// Signal agents to terminate gracefully
    Terminate { request_id: String },
}

/// Agent Phase for Replies/Status (from Agent)
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AgentPhase {
    #[default]
    Standby,
    Initializing,
    Initialized,
    BundlingConstructing,
    BundlingConstructingComplete(AddedTasks),
    ConflictResolving,
    ConflictResolvingComplete(ReleasedTasks),
    Terminating,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentStatus {
    pub agent_id: AgentId,
    pub phase: AgentPhase,
}

impl Keyed for AgentStatus {
    type K = u32;
    fn key(&self) -> Self::K {
        self.agent_id.0
    }
}

/// Reply to Syncer from Agents
/// Topic: AGENT_REPLY_TOPIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentReply {
    pub request_id: String,
    pub agent_id: AgentId,
    pub iteration: usize,
    pub phase: AgentPhase,
}

impl Keyed for AgentReply {
    type K = u32;
    fn key(&self) -> Self::K {
        self.agent_id.0
    }
}
