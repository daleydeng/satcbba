set shell := ["/bin/sh", "-c"]

ex3_agent := "ex3_dds_agent"
ex3_syncer := "ex3_dds_syncer"
ex3_runner := "ex3_dds_runner"
ex3_inproc_runner := "ex3_dds_inproc_runner"
ex2_agent := "ex2_dds_agent"
ex2_syncer := "ex2_dds_syncer"
ex2_runner := "ex2_dds_runner"
ex1_sim_sat := "ex1_sim_sat"

@default:
	just --list

build-ex1-sim-sat:
	@cargo build --release --example {{ex1_sim_sat}}

run-ex1-sim-sat config="examples/ex1/config.pkl": build-ex1-sim-sat
	@cargo run --release --example {{ex1_sim_sat}} -- --config {{config}}

build-ex3-deps:
	@cargo build --release --example {{ex3_agent}}
	@cargo build --release --example {{ex3_syncer}}

build-ex3-runner: build-ex3-deps
	@cargo build --release --example {{ex3_runner}}

run-ex3-runner agents='3' interval='3': build-ex3-runner
	@cargo run --release --example {{ex3_runner}} -- --agents {{agents}} --interval {{interval}}

run-ex3-agent agent_id='1':
	@cargo run --release --example {{ex3_agent}} -- {{agent_id}}

run-ex3-syncer interval='3':
	@cargo run --release --example {{ex3_syncer}} -- --interval {{interval}}

build-ex3-inproc-runner:
	@cargo build --release --example {{ex3_inproc_runner}}

run-ex3-inproc-runner: build-ex3-inproc-runner
	@cargo run --release --example {{ex3_inproc_runner}}

build-ex2-deps:
	@cargo build --release --example {{ex2_agent}}
	@cargo build --release --example {{ex2_syncer}}

build-ex2-runner: build-ex2-deps
	@cargo build --release --example {{ex2_runner}}

run-ex2-runner config="examples/ex2/config.pkl" terminate_agents='true': build-ex2-runner
	@cargo run --release --example {{ex2_runner}} -- --config {{config}} --terminate-agents={{terminate_agents}}

run-ex2-agent agent_id='1' config="examples/ex2/config.pkl":
	@cargo run --release --example {{ex2_agent}} -- {{agent_id}} --config {{config}}

run-ex2-syncer config="examples/ex2/config.pkl" terminate_agents='false':
	@cargo run --release --example {{ex2_syncer}} -- --config {{config}} --terminate-agents={{terminate_agents}}