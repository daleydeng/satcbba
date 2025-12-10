//! Satellite exploration example demonstrating CBAA usage with random generation and visualization

use cbbadds::sat::data::{SatGenParams, SourceMode, TaskGenParams};
use clap::Parser;
use serde::Deserialize;

use cbbadds::config::load_pkl;
use cbbadds::consensus::cbba::logging as cbba_log;
use cbbadds::logger;
use tracing::{info, warn};

use cbbadds::CBBA;
use cbbadds::cbba::Config as CBBAConfig;
use cbbadds::consensus::types::ConsensusMessage;
use cbbadds::sat::score::SatelliteScoreFunction;
use cbbadds::sat::viz::VizConfig;
use cbbadds::sat::{
    generate_random_satellites, generate_random_tasks, load_satellites, load_tasks,
    render_visualization, save_satellites, save_tasks,
};
use chrono::Local;
use std::path::Path;

#[derive(Debug, Deserialize, Clone)]
struct FileSourceConfig {
    path: String,
}

#[derive(Debug, Deserialize, Clone)]
struct SatSourceWrapper {
    mode: SourceMode,
    #[serde(default)]
    random: Option<SatGenParams>,
    #[serde(default)]
    file: Option<FileSourceConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "SatSourceWrapper")]
enum SatSourceConfig {
    Random(SatGenParams),
    File(FileSourceConfig),
}

impl From<SatSourceWrapper> for SatSourceConfig {
    fn from(w: SatSourceWrapper) -> Self {
        match w.mode {
            SourceMode::Random => SatSourceConfig::Random(w.random.unwrap_or(SatGenParams {
                count: 5,
                ..Default::default()
            })),
            SourceMode::File => {
                SatSourceConfig::File(w.file.expect("Missing file config for file mode"))
            }
        }
    }
}

impl Default for SatSourceConfig {
    fn default() -> Self {
        SatSourceConfig::Random(SatGenParams {
            count: 5,
            ..Default::default()
        })
    }
}

#[derive(Debug, Deserialize, Clone)]
struct TaskSourceWrapper {
    mode: SourceMode,
    #[serde(default)]
    random: Option<TaskGenParams>,
    #[serde(default)]
    file: Option<FileSourceConfig>,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(from = "TaskSourceWrapper")]
enum TaskSourceConfig {
    Random(TaskGenParams),
    File(FileSourceConfig),
}

impl From<TaskSourceWrapper> for TaskSourceConfig {
    fn from(w: TaskSourceWrapper) -> Self {
        match w.mode {
            SourceMode::Random => TaskSourceConfig::Random(w.random.unwrap_or(TaskGenParams {
                count: 10,
                ..Default::default()
            })),
            SourceMode::File => {
                TaskSourceConfig::File(w.file.expect("Missing file config for file mode"))
            }
        }
    }
}

impl Default for TaskSourceConfig {
    fn default() -> Self {
        TaskSourceConfig::Random(TaskGenParams {
            count: 10,
            ..Default::default()
        })
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct DataConfig {
    satellites: SatSourceConfig,
    tasks: TaskSourceConfig,
}

#[derive(Debug, Deserialize)]
#[serde(default)]
struct OutputConfig {
    dir: String,
    use_timestamp: bool,
    timestamp_fmt: String,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            dir: "results".to_string(),
            use_timestamp: true,
            timestamp_fmt: "%Y-%m-%d_%H-%M-%S".to_string(),
        }
    }
}

#[derive(Default, Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
enum AlgorithmMethod {
    #[default]
    CBBA,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct AlgoConfig {
    method: AlgorithmMethod,
    cbba: CBBAConfig,
}

#[derive(Default, Debug, Deserialize)]
#[serde(default)]
struct SimConfig {
    data: DataConfig,
    viz: VizConfig,
    output_config: OutputConfig,
    algo: AlgoConfig,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
struct Cli {
    /// Path to configuration file
    #[arg(long, default_value = "examples/ex1/config.pkl")]
    config: String,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Parse command line arguments
    let cli = Cli::parse();

    // Load config first to determine output directory
    let config_path = &cli.config;
    let config: SimConfig = match load_pkl(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Failed to load pkl config {}: {}", config_path, e);
            return Err(e.into());
        }
    };

    // Setup output directory
    let output_dir = &config.output_config.dir;
    let use_timestamp = config.output_config.use_timestamp;
    let timestamp_fmt = &config.output_config.timestamp_fmt;

    let result_dir = if use_timestamp {
        let date_str = Local::now().format(timestamp_fmt).to_string();
        Path::new(output_dir).join(date_str)
    } else {
        Path::new(output_dir).to_path_buf()
    };

    std::fs::create_dir_all(&result_dir)?;

    // Setup logging
    let log_path = result_dir.join("simulation.log");
    let _guard = logger::init(log_path, "info")?;

    info!("Loaded configuration from {}", config_path);
    info!("Results will be saved to: {}", result_dir.display());

    // Load Satellites
    let agents = match &config.data.satellites {
        SatSourceConfig::Random(cfg) => {
            info!("Generating {} random satellites", cfg.count);
            let sats = generate_random_satellites(cfg);
            // Save generated satellites
            save_satellites(&sats, &result_dir.join("satellites.json"))?;
            sats
        }
        SatSourceConfig::File(cfg) => {
            info!("Loading satellites from file: {}", cfg.path);
            let path = Path::new(&cfg.path);
            // Copy file to result dir
            if let Err(e) = std::fs::copy(path, result_dir.join("satellites.json")) {
                warn!("Warning: Failed to copy satellites file: {}", e);
            }
            load_satellites(path)?
        }
    };

    // Load Tasks
    let tasks = match &config.data.tasks {
        TaskSourceConfig::Random(cfg) => {
            info!("Generating {} random tasks", cfg.count);
            let tasks = generate_random_tasks(cfg);
            // Save generated tasks
            save_tasks(&tasks, &result_dir.join("tasks.json"))?;
            tasks
        }
        TaskSourceConfig::File(cfg) => {
            info!("Loading tasks from file: {}", cfg.path);
            let path = Path::new(&cfg.path);
            // Copy file to result dir
            if let Err(e) = std::fs::copy(path, result_dir.join("tasks.json")) {
                warn!("Warning: Failed to copy tasks file: {}", e);
            }
            load_tasks(path)?
        }
    };

    let mut cbbas = Vec::new();

    // Create CBBA configuration
    let cbba_config = config.algo.cbba.clone();

    // Create CBBA instances for each agent
    for agent in agents {
        let score_function = SatelliteScoreFunction;
        let cbba = CBBA::new(
            agent,
            score_function,
            tasks.clone(),
            Some(cbba_config.clone()),
        );
        cbbas.push(cbba);
    }

    // Run simulation
    let mut iteration = 0;
    let mut converged = false;

    info!("Running CBBA iterations...\n");

    // Visualize initial state (Iteration 0)
    render_visualization(
        &result_dir.join(format!("iteration_{}.png", 0)),
        "Iteration 0",
        &cbbas,
        &tasks,
        &config.viz,
    )?;

    while !converged && iteration < 100 {
        iteration += 1;
        info!("--- Iteration {} ---", iteration);

        // Track convergence for this iteration
        // If NO agent changes its bundle or bids, we are converged.
        // However, CBBA convergence is technically when bundles stop changing.
        // But bid updates might continue for a bit.
        // Let's track if any agent modifies its bundle OR its internal state significantly.
        let mut any_change = false;

        // Phase 1: Bundle Construction
        for cbba in &mut cbbas {
            let added = cbba.bundle_construction_phase(Some(&tasks))?;
            if !added.is_empty() {
                any_change = true;
                // info!("  {} added tasks: {}", cbba.agent, added);
            }
        }

        // Phase 2: Exchange winning information
        let mut all_messages = Vec::new();
        for cbba in &cbbas {
            if !cbba.bids.is_empty() {
                all_messages.push(ConsensusMessage {
                    agent_id: cbba.agent.id,
                    bids: cbba.bids.clone(),
                });
            }
        }

        // Apply consensus / conflict resolution
        for cbba in &mut cbbas {
            let dropped = cbba.consensus_phase(&all_messages);
            if !dropped.is_empty() {
                any_change = true;
                cbba_log::log_dropped_tasks(cbba, &dropped);
            }
        }

        if !any_change {
            converged = true;
        }

        // Visualize this iteration

        // Visualize this iteration
        render_visualization(
            &result_dir.join(format!("iteration_{}.png", iteration)),
            &format!("Iteration {}", iteration),
            &cbbas,
            &tasks,
            &config.viz,
        )?;

        // Print status for each agent at the end of the iteration
        cbba_log::log_iteration_status(&cbbas, iteration);
    }

    cbba_log::log_final_status(&cbbas);
    cbba_log::log_assignment_table(&cbbas, &tasks);

    info!("Visualization saved to {}", result_dir.display());

    Ok(())
}
