use crate::consensus::types::{Agent, AgentId, Task, TaskId};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Use integer microdegrees (degrees * 1e6) for lat/lon in structs so they remain `Hash`/`Eq`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ExploreTask {
    pub id: TaskId,
    pub lat_e6: i32,
    pub lon_e6: i32,
    /// decay rate per hour used in exp(-decay * travel_time_hours) for this task
    pub decay_rate_per_hr: f64,
    pub base_score: f64,
    /// Duration required to execute the task in seconds
    #[serde(default)]
    pub execution_duration_sec: f64,
    pub allowed_satellites: Option<HashSet<AgentId>>,
}

use crate::error::Error;

impl ExploreTask {
    pub fn new(
        id: TaskId,
        lat_e6: i32,
        lon_e6: i32,
        decay_rate_per_hr: f64,
        base_score: f64,
        execution_duration_sec: f64,
        allowed_satellites: Option<HashSet<AgentId>>,
    ) -> Result<Self, Error> {
        if decay_rate_per_hr < 0.0 {
            return Err(Error::ValidationError(
                "decay_rate_per_hr must be non-negative".to_string(),
            ));
        }
        if base_score < 0.0 {
            return Err(Error::ValidationError(
                "base_score must be non-negative".to_string(),
            ));
        }
        if execution_duration_sec < 0.0 {
            return Err(Error::ValidationError(
                "execution_duration_sec must be non-negative".to_string(),
            ));
        }
        Ok(Self {
            id,
            lat_e6,
            lon_e6,
            decay_rate_per_hr,
            base_score,
            execution_duration_sec,
            allowed_satellites,
        })
    }
}

impl std::fmt::Display for ExploreTask {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "T{}", self.id)
    }
}

impl PartialEq for ExploreTask {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for ExploreTask {}

impl PartialOrd for ExploreTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for ExploreTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.id.cmp(&other.id)
    }
}

impl Task for ExploreTask {
    fn id(&self) -> TaskId {
        self.id
    }
}

impl std::hash::Hash for ExploreTask {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct Satellite {
    pub id: AgentId,
    pub lat_e6: i32,
    pub lon_e6: i32,
    pub speed_kmph: u32,
}

impl Agent for Satellite {
    fn id(&self) -> AgentId {
        self.id
    }
}

impl std::fmt::Display for Satellite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "A{}", self.id)
    }
}
