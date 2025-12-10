use super::types::{ExploreTask, Satellite};
use super::utils::{deg_from_e6, haversine_km};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct SatelliteScoreFunction;

use crate::consensus::types::Score;

impl crate::cbaa::ScoreFunction<ExploreTask, Satellite> for SatelliteScoreFunction {
    fn check(&self, agent: &Satellite, task: &ExploreTask) -> bool {
        if let Some(allowed) = &task.allowed_satellites
            && !allowed.contains(&agent.id)
        {
            return false;
        }
        true
    }

    fn calc(&self, agent: &Satellite, task: &ExploreTask) -> Score {
        let lat_a = deg_from_e6(agent.lat_e6);
        let lon_a = deg_from_e6(agent.lon_e6);
        let lat_t = deg_from_e6(task.lat_e6);
        let lon_t = deg_from_e6(task.lon_e6);

        let distance_km = haversine_km(lat_a, lon_a, lat_t, lon_t);

        let final_score = 10000.0 / (100.0 + distance_km);

        if final_score < 0.0 {
            Score::default()
        } else {
            // Ensure minimum score of 1 for valid tasks to distinguish from Zero (invalid)
            Score::new(final_score.round().max(1.0) as u32)
        }
    }
}

impl crate::cbba::ScoreFunction<ExploreTask, Satellite> for SatelliteScoreFunction {
    fn check(&self, agent: &Satellite, task: &ExploreTask) -> bool {
        if let Some(allowed) = &task.allowed_satellites
            && !allowed.contains(&agent.id)
        {
            return false;
        }
        true
    }

    fn calc_path(&self, agent: &Satellite, path: &[ExploreTask]) -> Vec<Score> {
        let mut scores = Vec::with_capacity(path.len());
        let mut current_time_hours = 0.0;
        let mut current_lat = deg_from_e6(agent.lat_e6);
        let mut current_lon = deg_from_e6(agent.lon_e6);

        for task in path {
            let target_lat = deg_from_e6(task.lat_e6);
            let target_lon = deg_from_e6(task.lon_e6);

            let distance_km = haversine_km(current_lat, current_lon, target_lat, target_lon);
            let travel_time = distance_km / agent.speed_kmph as f64;

            current_time_hours += travel_time;

            // Score = BaseValue * e^(-lambda * arrival_time)
            let lambda = task.decay_rate_per_hr;
            let base_value = task.base_score;

            let final_score = base_value * (-lambda * current_time_hours).exp();

            if final_score < 0.0 {
                panic!("Calculated score is negative: {}", final_score);
            } else {
                // Ensure minimum score of 1 for valid tasks
                scores.push(Score::new(final_score.round().max(1.0) as u32));
            }

            // Add execution time of the current task before moving to the next one
            current_time_hours += task.execution_duration_sec / 3600.0;

            current_lat = target_lat;
            current_lon = target_lon;
        }

        scores
    }
}
