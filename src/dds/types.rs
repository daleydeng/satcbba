use serde::{Deserialize, Serialize};
use crate::consensus::types::{AgentId, Timestamp};
use rustdds::Keyed;

// --- Topic Names ---
pub const REQUEST_FROM_MANAGER_TOPIC: &str = "RequestFromManager";
pub const REPLY_TO_MANAGER_TOPIC: &str = "ReplyToManager";
pub const REQUEST_FROM_SYNCER_TOPIC: &str = "RequestFromSyncer";
pub const REPLY_TO_SYNCER_TOPIC: &str = "ReplyToSyncer";
pub const AGENT_STATE_TOPIC: &str = "AgentState"; // Keyed
pub const AGENT_STATUS_TOPIC: &str = "AgentStatus"; // Keyed
pub const AGENT_CONSENSUS_TOPIC: &str = "AgentConsensus"; // Keyed
pub const LOG_TOPIC: &str = "SystemLog";

// --- Common / Shared Types ---

/// Agent Phase for Requests (from Syncer/Manager)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum AgentRequestPhase {
    Initializing,
    BundleConstruction,
    ConflictResolution,
}

/// Agent Phase for Replies/Status (from Agent)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Default)]
pub enum AgentPhase {
    #[default]
    Standby,
    Initializing,
    Initialized,
    BundleConstruction,
    BundleConstructionComplete(bool), // has_changes
    ConflictResolution,
    ConflictResolutionComplete(bool), // has_changes
}



/// Log Message for centralized logging
/// Direction: All -> Manager
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    pub timestamp: Timestamp,
    pub source: String,
    pub level: String, // "INFO", "WARN", "ERROR"
    pub content: String,
}

// --- DDS Message Payloads ---

/// Agent Status Message (formerly SyncMessage)
/// Direction: Agent -> Syncer & Manager
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

/// Phase Control Message from Syncer to Agents
/// Direction: Syncer -> Agent
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentInstruction {
    pub request_id: String,
    pub target_phase: AgentRequestPhase,
}

/// Commands from Manager to All Components
/// Direction: Manager -> Agent & Syncer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ManagerCommand {
    Start,
    Stop,
    Reset,
    Shutdown,
    StartStep(AgentInstruction),
    ClearTasks,
}

// --- Unified Message Types (Topic Payloads) ---

/// Request from Manager to Syncer/Agents
/// Topic: REQUEST_FROM_MANAGER_TOPIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestFromManager<T> {
    Command {
        id: String,
        command: ManagerCommand,
    },
    DistributeTasks {
        id: String,
        tasks: Vec<T>,
    },
}

/// Reply to Manager from Syncer/Agents
/// Topic: REPLY_TO_MANAGER_TOPIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyToManager {
    pub request_id: String,
    pub success: bool,
    pub message: String,
    // Optional status data
    pub iteration: Option<usize>,
    pub phase: Option<AgentPhase>,
    pub active_agents: Option<u32>,
    pub converged: Option<bool>,
}

/// Request from Syncer to Agents
/// Topic: REQUEST_FROM_SYNCER_TOPIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RequestFromSyncer<T> {
    Instruction(AgentInstruction),
    ForwardTasks(Vec<T>),
    ClearTasks,
}

/// Reply to Syncer from Agents
/// Topic: REPLY_TO_SYNCER_TOPIC
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplyToSyncer {
    pub request_id: String,
    pub agent_id: AgentId,
    pub iteration: usize,
    pub phase: AgentPhase,
}

impl Keyed for ReplyToSyncer {
    type K = u32;
    fn key(&self) -> Self::K {
        self.agent_id.0
    }
}
