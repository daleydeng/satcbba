//! Core types and traits for the auction algorithms

use std::collections::HashMap;
use std::fmt::Debug;
use std::hash::Hash;
use std::time::{SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};

use std::num::NonZeroU32;
use rustdds::Keyed;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash)]
#[derive(Default)]
pub enum Score {
    #[default]
    Zero,
    Positive(NonZeroU32),
}


impl Score {
    pub fn new(value: u32) -> Self {
        match NonZeroU32::new(value) {
            Some(v) => Self::Positive(v),
            None => Self::Zero,
        }
    }

    pub fn val(&self) -> u32 {
        match self {
            Self::Zero => 0,
            Self::Positive(v) => v.get(),
        }
    }

    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Zero)
    }

    pub fn is_positive(&self) -> bool {
        matches!(self, Self::Positive(_))
    }
}

impl Score {
    pub fn saturating_sub(&self, other: Self) -> Self {
        Self::new(self.val().saturating_sub(other.val()))
    }
}

impl std::fmt::Display for Score {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.val())
    }
}

impl From<u32> for Score {
    fn from(v: u32) -> Self {
        Self::new(v)
    }
}

impl From<Score> for u32 {
    fn from(s: Score) -> Self {
        s.val()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct TaskId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConvergenceStatus {
    Converged,
    MaxIterationsReached,
    Running,
}

impl std::fmt::Display for TaskId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, PartialOrd, Ord)]
pub struct AgentId(pub u32);

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, Hash, Default)]
pub struct Timestamp(pub u64);

impl std::fmt::Display for Timestamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<u64> for Timestamp {
    fn from(v: u64) -> Self {
        Timestamp(v)
    }
}

impl From<Timestamp> for u64 {
    fn from(t: Timestamp) -> Self {
        t.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BidInfo {
    Winner(AgentId, Score, Timestamp),
    None(Timestamp),
}

impl BidInfo {
    pub fn winner(agent_id: AgentId, score: Score) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        Self::Winner(agent_id, score, Timestamp(now))
    }

    pub fn winner_with_timestamp(agent_id: AgentId, score: Score, timestamp: Timestamp) -> Self {
        Self::Winner(agent_id, score, timestamp)
    }

    pub fn none() -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_micros() as u64;
        Self::None(Timestamp(now))
    }

    pub fn none_with_timestamp(timestamp: Timestamp) -> Self {
        Self::None(timestamp)
    }

    /// Creates a None bid with timestamp 0, representing the initial state
    pub fn none_initial() -> Self {
        Self::None(Timestamp(0))
    }
}

/// Message types for consensus communication
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConsensusMessage {
    pub agent_id: AgentId,
    pub bids: HashMap<TaskId, BidInfo>,
}

impl Keyed for ConsensusMessage {
    type K = u32;
    fn key(&self) -> Self::K {
        self.agent_id.0
    }
}

/// Core trait that task types must implement
pub trait Task: Clone + Debug + PartialEq {
    fn id(&self) -> TaskId;
}

/// Core trait for agents
pub trait Agent: Debug {
    fn id(&self) -> AgentId;
}

/// A vector with a maximum size capacity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BoundedSortedSet<T> {
    data: Vec<T>,
    capacity: usize,
}

use crate::error::Error;

impl<T> BoundedSortedSet<T> {
    pub fn new(capacity: usize) -> Self {
        let actual_capacity = if capacity == 0 { usize::MAX } else { capacity };
        Self {
            data: if actual_capacity == usize::MAX { Vec::new() } else { Vec::with_capacity(actual_capacity) },
            capacity: actual_capacity,
            }
    }

    pub fn push(&mut self, item: T) -> Result<(), Error>
    where
        T: PartialEq,
    {
        if self.data.contains(&item) {
             return Err(Error::ItemAlreadyExists);
        }
        if self.capacity != usize::MAX && self.data.len() >= self.capacity {
             return Err(Error::CapacityFull);
        }

        self.data.push(item);
        Ok(())
    }

    pub fn insert(&mut self, index: usize, item: T) -> Result<(), Error>
    where
        T: PartialEq,
    {
        if self.data.contains(&item) {
             return Err(Error::ItemAlreadyExists);
        }
        if self.capacity != usize::MAX && self.data.len() >= self.capacity {
             return Err(Error::CapacityFull);
        }
        if index > self.data.len() {
             return Err(Error::IndexOutOfBounds);
        }

        self.data.insert(index, item);
        Ok(())
    }

    pub fn remove(&mut self, index: usize) -> Option<T> {
        if index < self.data.len() {
            Some(self.data.remove(index))
        } else {
            None
        }
    }

    pub fn retain<F>(&mut self, f: F)
    where
        F: FnMut(&T) -> bool,
    {
        self.data.retain(f);
    }

    pub fn truncate(&mut self, len: usize) {
        self.data.truncate(len);
    }

    pub fn clear(&mut self) {
        self.data.clear();
    }

    pub fn len(&self) -> usize {
        self.data.len()
    }

    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    pub fn is_full(&self) -> bool {
        if self.capacity == usize::MAX {
            false
        } else {
            self.data.len() >= self.capacity
        }
    }

    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.data.iter()
    }

    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    pub fn contains(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        self.data.contains(item)
    }

    pub fn find<P>(&self, predicate: P) -> Option<&T>
    where
        P: FnMut(&&T) -> bool,
    {
        self.data.iter().find(predicate)
    }

    /// Remove an item by value
    pub fn remove_item(&mut self, item: &T) -> bool
    where
        T: PartialEq,
    {
        let original_len = self.data.len();
        self.data.retain(|x| x != item);
        self.data.len() < original_len
    }

    /// Remove items after a specific item
    pub fn remove_items_after(&mut self, item: &T)
    where
        T: PartialEq,
    {
        if let Some(pos) = self.data.iter().position(|x| x == item) {
            self.data.truncate(pos);
        }
    }
}

impl<T> IntoIterator for BoundedSortedSet<T> {
    type Item = T;
    type IntoIter = std::vec::IntoIter<T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.into_iter()
    }
}

impl<'a, T> IntoIterator for &'a BoundedSortedSet<T> {
    type Item = &'a T;
    type IntoIter = std::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.iter()
    }
}

