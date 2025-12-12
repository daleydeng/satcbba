use rustdds::{GUID, Timestamp, Duration};
use rand::Rng;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TaskId(pub u32);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Score(pub u32);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct AgentId(pub GUID);

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct ObWindow {
    pub start: Timestamp,
    pub end: Timestamp,
}

impl ObWindow {
    /// Returns true if this window intersects `other`.
    pub fn intersects(&self, other: &ObWindow) -> bool {
        !(self.end <= other.start || self.start >= other.end)
    }

    /// Returns true if this window intersects any window in `existings`.
    pub fn overlaps_any(&self, existings: &[ObWindow]) -> bool {
        existings.iter().any(|o| self.intersects(o))
    }

    /// Randomly generate an `ObWindow` relative to now using provided ranges.
    /// - `start_offset_secs_range`: range for seconds offset from now for the start.
    /// - `length_secs_range`: range for the duration length in seconds.
    pub fn random_with(
        start_offset_secs_range: std::ops::Range<i32>,
        length_secs_range: std::ops::Range<i32>,
    ) -> ObWindow {
        let mut rng = rand::rng();
        let start = Timestamp::now() + Duration::from_secs(rng.random_range(start_offset_secs_range));
        let end = start + Duration::from_secs(rng.random_range(length_secs_range));
        ObWindow { start, end }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinnerId {
    Sure(AgentId),
    None,
    Renew,
}

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct WinningMessage {
    pub sender_id: AgentId,
    pub task_id: TaskId,
    pub winner_id: WinnerId,
    pub score: Score,
    pub timestamp: Timestamp,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Default)]
pub struct WinnersMessage {
    pub sender_id: AgentId,
    pub winners: Vec<WinningMessage>,
}


#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum NTPInfo {
    ServerResponse(Timestamp),
    ClientRequest,
}

pub type TaskProfile = (TaskId, (f64, f64));

#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq)]
pub enum UpdateInfo {
    Add(TaskProfile),
    Remove(TaskId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Player {
    Server,
    Client,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TaskInfo {
    pub id: TaskId,
    pub score: Score,
    pub ob_window: ObWindow,
}
