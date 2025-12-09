use std::fs;
use std::path::Path;
use serde::{Serialize, Deserialize};
use serde_json;
use rand::Rng;
use super::types::{ExploreTask, Satellite};
use crate::consensus::types::{TaskId, AgentId};
use anyhow::{Result, Context};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SourceMode {
    Random,
    File,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct TaskGenParams {
    pub count: usize,
    pub lat_range: Option<(i32, i32)>,
    pub lon_range: Option<(i32, i32)>,
    pub decay_rate_range: Option<(f64, f64)>,
    pub score_range: Option<(f64, f64)>,
    pub duration_range: Option<(f64, f64)>,
}

#[derive(Default, Debug, Clone, Serialize, Deserialize)]
pub struct SatGenParams {
    pub count: usize,
    pub lat_range: Option<(i32, i32)>,
    pub lon_range: Option<(i32, i32)>,
    pub speed: Option<u32>,
}

pub fn generate_random_tasks(params: &TaskGenParams) -> Vec<ExploreTask> {
    let mut rng = rand::rng();
    let mut tasks = Vec::new();

    let (min_lat, max_lat) = params.lat_range.unwrap_or((-90_000_000, 90_000_000));
    let (min_lon, max_lon) = params.lon_range.unwrap_or((-180_000_000, 180_000_000));
    let (min_decay, max_decay) = params.decay_rate_range.unwrap_or((0.1, 1.0));
    let (min_score, max_score) = params.score_range.unwrap_or((100.0, 1000.0));
    let (min_duration, max_duration) = params.duration_range.unwrap_or((60.0, 600.0));

    for i in 1..=params.count {
        if let Ok(task) = ExploreTask::new(
            TaskId(i as u32),
            rng.random_range(min_lat..max_lat),
            rng.random_range(min_lon..max_lon),
            rng.random_range(min_decay..max_decay),
            rng.random_range(min_score..max_score),
            rng.random_range(min_duration..max_duration), // Random execution duration between 1 and 10 minutes
            None,
        ) {
            tasks.push(task);
        }
    }
    tasks
}

pub fn generate_random_satellites(params: &SatGenParams) -> Vec<Satellite> {
    let mut rng = rand::rng();
    let mut sats = Vec::new();

    let (min_lat, max_lat) = params.lat_range.unwrap_or((-90_000_000, 90_000_000));
    let (min_lon, max_lon) = params.lon_range.unwrap_or((-180_000_000, 180_000_000));
    let speed = params.speed.unwrap_or(28000);

    for i in 1..=params.count {
        sats.push(Satellite {
            id: AgentId(i as u32),
            lat_e6: rng.random_range(min_lat..max_lat),
            lon_e6: rng.random_range(min_lon..max_lon),
            speed_kmph: speed,
        });
    }
    sats
}

pub fn load_tasks(path: &Path) -> Result<Vec<ExploreTask>> {
    let json = fs::read_to_string(path).context("Failed to read tasks file")?;
    let tasks: Vec<ExploreTask> = serde_json::from_str(&json).context("Failed to parse tasks json")?;
    Ok(tasks)
}

pub fn load_satellites(path: &Path) -> Result<Vec<Satellite>> {
    let json = fs::read_to_string(path).context("Failed to read satellites file")?;
    let sats: Vec<Satellite> = serde_json::from_str(&json).context("Failed to parse satellites json")?;
    Ok(sats)
}

pub fn save_tasks(tasks: &[ExploreTask], path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(tasks).context("Failed to serialize tasks")?;
    fs::write(path, json).context("Failed to write tasks file")?;
    Ok(())
}

pub fn save_satellites(sats: &[Satellite], path: &Path) -> Result<()> {
    let json = serde_json::to_string_pretty(sats).context("Failed to serialize satellites")?;
    fs::write(path, json).context("Failed to write satellites file")?;
    Ok(())
}

pub fn generate_and_save_data(task_params: &TaskGenParams, sat_params: &SatGenParams, tasks_path: &Path, agents_path: &Path) -> Result<()> {
    let tasks = generate_random_tasks(task_params);
    let sats = generate_random_satellites(sat_params);

    save_tasks(&tasks, tasks_path)?;
    save_satellites(&sats, agents_path)?;

    Ok(())
}
