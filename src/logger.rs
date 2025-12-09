use std::path::Path;
use chrono::Local;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, Layer};
use tracing_subscriber::fmt::time::FormatTime;
use tracing_appender::non_blocking::WorkerGuard;

#[derive(Clone, Copy)]
struct CustomTimer;

impl FormatTime for CustomTimer {
    fn format_time(&self, w: &mut tracing_subscriber::fmt::format::Writer<'_>) -> std::fmt::Result {
        let now = Local::now();
        write!(w, "{}", now.format("%m%dT%H:%M:%S%.3f"))
    }
}

pub fn init(log_path: impl AsRef<Path>, level: &str) -> Result<WorkerGuard, Box<dyn std::error::Error>> {
    let file = std::fs::File::create(log_path)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(file);

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(CustomTimer)
                .with_writer(std::io::stdout)
                .with_filter(tracing_subscriber::EnvFilter::new(level))
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_timer(CustomTimer)
                .with_writer(non_blocking)
                .with_ansi(false)
                .with_filter(tracing_subscriber::EnvFilter::new(level))
        )
        .init();

    Ok(guard)
}
