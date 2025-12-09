use serde::Deserialize;
use cbbadds::cbba::Config as CBBAConfig;
use cbbadds::sat::data::{TaskGenParams, SatGenParams, SourceMode};
use cbbadds::sat::viz::VizConfig;

#[derive(Debug, Deserialize, Clone)]
pub struct DdsConfig {
    pub domain_id: u16,
    pub num_agents: usize,
    #[serde(default)]
    pub expected_agents: usize, // For server to know how many to wait for
}

#[derive(Debug, Deserialize, Clone)]
pub struct FileSourceConfig {
    pub path: String,
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
pub enum SatSourceConfig {
    Random(SatGenParams),
    File(FileSourceConfig),
}

impl From<SatSourceWrapper> for SatSourceConfig {
    fn from(w: SatSourceWrapper) -> Self {
        match w.mode {
            SourceMode::Random => SatSourceConfig::Random(w.random.unwrap_or(SatGenParams { count: 5, ..Default::default() })),
            SourceMode::File => SatSourceConfig::File(w.file.expect("Missing file config for file mode")),
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
pub enum TaskSourceConfig {
    Random(TaskGenParams),
    File(FileSourceConfig),
}

impl From<TaskSourceWrapper> for TaskSourceConfig {
    fn from(w: TaskSourceWrapper) -> Self {
        match w.mode {
            SourceMode::Random => TaskSourceConfig::Random(w.random.unwrap_or(TaskGenParams { count: 10, ..Default::default() })),
            SourceMode::File => TaskSourceConfig::File(w.file.expect("Missing file config for file mode")),
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

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default)]
pub struct DataConfig {
    pub satellites: SatSourceConfig,
    pub tasks: TaskSourceConfig,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct OutputConfig {
    pub dir: String,
    pub use_timestamp: bool,
    pub timestamp_fmt: String,
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
pub enum AlgorithmMethod {
    #[default]
    CBBA,
}

#[derive(Debug, Default, Deserialize, Clone)]
#[serde(default)]
pub struct AlgoConfig {
    pub method: AlgorithmMethod,
    pub cbba: CBBAConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub dds: DdsConfig,

    #[serde(default)]
    pub data: DataConfig,

    #[serde(default)]
    pub viz: VizConfig,

    #[serde(default)]
    pub output_config: OutputConfig,

    #[serde(default)]
    pub algo: AlgoConfig,
}
