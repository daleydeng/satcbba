use clap::Parser;
use std::process::{Child, Command};
use std::thread;
use std::time::Duration;

#[derive(Parser, Debug)]
#[command(author, version, about = "Launch ex3 syncer and agents", long_about = None)]
struct Cli {
    /// Interval in seconds between syncer pings
    #[arg(long, default_value_t = 3)]
    interval: u64,

    /// Number of agents to spawn
    #[arg(long, default_value_t = 3)]
    agents: u32,
}

fn main() {
    let cli = Cli::parse();
    println!(
        "ex3_dds_runner: launching 1 syncer (interval={}s) and {} agents as child processes...",
        cli.interval, cli.agents
    );

    let current_exe = std::env::current_exe().expect("Failed to resolve current exe path");
    let exe_dir = current_exe
        .parent()
        .expect("Current executable should have a parent directory");

    // Launch agents first so the syncer starts after all agents are up
    let agent_path = exe_dir.join("ex3_dds_agent");
    let mut agents: Vec<Child> = Vec::new();
    for i in 1..=cli.agents {
        let child = Command::new(&agent_path)
            .arg(i.to_string())
            .spawn()
            .unwrap_or_else(|e| panic!("Failed to start {:?} for agent {}: {}", agent_path, i, e));
        agents.push(child);
        thread::sleep(Duration::from_millis(300)); // Stagger startup
    }

    // Give agents a brief moment to come online, then launch syncer
    thread::sleep(Duration::from_secs(1));

    let syncer_path = exe_dir.join("ex3_dds_syncer");
    let mut syncer = Command::new(&syncer_path)
        .arg("--interval")
        .arg(cli.interval.to_string())
        .spawn()
        .unwrap_or_else(|e| panic!("Failed to start {:?}: {}", syncer_path, e));

    println!("All processes launched. Press Ctrl+C to exit.");

    // Wait for children (will run forever)
    let _ = syncer.wait();
    for mut agent in agents {
        let _ = agent.wait();
    }
}
