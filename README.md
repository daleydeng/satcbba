# satcbba：Rust 分布式任务分配与仿真库

[![Crates.io](https://img.shields.io/crates/v/satcbba.svg)](https://crates.io/crates/satcbba)
[![Documentation](https://docs.rs/satcbba/badge.svg)](https://docs.rs/satcbba)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](https://github.com/daleydeng/satcbba)

satcbba（crate 名称为 `satcbba`）提供共识拍卖算法（CBAA）与共识包算法（CBBA）的 Rust 实现，适用于多智能体的分布式任务分配场景。仓库还包含基于 DDS（Data Distribution Service）的仿真示例，方便快速验证通信与算法行为。

## 致谢

本项目基于以下经典论文的算法思想实现：

> H.-L. Choi, L. Brunet, and J. P. How,
> *Consensus-Based Decentralized Auctions for Robust Task Allocation,*
> **IEEE Transactions on Robotics**, vol. 25, no. 4, pp. 912-926, Aug. 2009.
> [DOI: 10.1109/TRO.2009.2022423](https://doi.org/10.1109/TRO.2009.2022423)

代码完全从零编写，不包含原论文中的任何代码或插图，仅复现算法思路。如需在学术成果中引用，请注明上述论文。

## 名称由来

名称 “satcbba” 代表 “Satellite Consensus-Based Bundle Auction”，凸显仓库聚焦于卫星场景下的分布式任务分配与拍卖算法。

## 功能亮点

- **CBAA**：针对单任务分配的共识拍卖算法实现。
- **CBBA**：支持多任务捆绑的共识包算法。
- **通用接口**：任务、智能体类型可自由扩展，成本函数与网络拓扑均可自定义。
- **异步友好**：核心设计兼容 Rust `async/await` 模式。
- **轻量依赖**：核心最小化，额外能力通过可选模块提供。

## 安装

在项目的 `Cargo.toml` 中加入：

```toml
[dependencies]
satcbba = "0.1.0"
```

如果仅复用库而不运行示例，直接依赖 crate 即可；仓库内的示例与工具无需额外配置。

## 快速上手

### CBAA（单任务分配）示例

```rust
use satcbba::*;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Task(u32);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
struct Agent(u32);

/// 简单示例：任务编号越小越优先
struct SimpleCostFunction;

impl CostFunction<Task, Agent> for SimpleCostFunction {
    fn calculate_cost(&self, _agent: Agent, task: &Task, _ctx: &AssignmentContext<Task, Agent>) -> f64 {
        100.0 - task.0 as f64
    }
}

let agent_id = Agent(1);
let cost_function = SimpleCostFunction;
let available_tasks = vec![Task(1), Task(2), Task(3)];

let mut cbaa = CBAA::new(agent_id, cost_function, available_tasks, None);

let _ = cbaa.auction_phase()?; // 拍卖阶段
let messages = Vec::new();      // 从网络层获取消息
let _ = cbaa.consensus_phase(messages)?; // 共识阶段

if let Some(assignment) = cbaa.get_assignment() {
    println!("Agent {} => Task {}", agent_id, assignment);
}
```

### CBBA（多任务分配）示例

```rust
use satcbba::*;
use satcbba::cbba::ScoreFunction;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
struct MultiTask(u32);
impl Task for MultiTask {
    fn id(&self) -> TaskId { TaskId(self.0) }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize, PartialOrd, Ord)]
struct MultiAgent(u32);
impl Agent for MultiAgent {
    fn id(&self) -> AgentId { AgentId(self.0) }
}

struct MultiScoreFunction;

impl ConstraintChecker<MultiTask, MultiAgent> for MultiScoreFunction {
    fn check(&self, _agent: &MultiAgent, _task: &MultiTask) -> bool { true }
}

impl ScoreFunction<MultiTask, MultiAgent> for MultiScoreFunction {
    fn calc(&self, _agent: &MultiAgent, task: &MultiTask, _path: &[MultiTask], _pos: usize) -> Score {
        Score(task.0)
    }
}

let cfg = AuctionConfig::cbba(3); // 每个智能体最多可领取 3 个任务
let mut cbba = CBBA::new(
    MultiAgent(1),
    MultiScoreFunction,
    vec![MultiTask(1), MultiTask(2), MultiTask(3)],
    Some(cfg),
);

let added_tasks = cbba.bundle_construction_phase();
```

## 目录结构概览

- **`types`**：核心类型与 trait（如 `Task`、`AgentId`、`CostFunction`）。
- **`error`**：统一的错误类型。
- **`config`**：算法配置结构体。
- **`consensus`**：CBAA/CBBA 及相关数据结构实现。
- **`dds`**：基于 RustDDS 的传输工具与辅助类型。
- **`sat`**：卫星任务仿真逻辑与可视化工具。

## DDS 分布式仿真指南

仓库提供基于 DDS 的分布式运行模式：服务器负责同步节奏与可视化，各个卫星（Agent）作为独立进程执行拍卖逻辑。

### 运行前提

- 已安装 Rust toolchain。
- 网络接口支持 multicast。示例默认使用 RustDDS，无需安装额外中间件。

### 核心组件

1. **DDS Server (`examples/dds_server.rs`)**
   - 加载任务与卫星配置。
   - 发布任务与控制信号（开始、迭代）。
   - 收集 Agent 状态并输出汇总。

2. **DDS Agent (`examples/dds_agent.rs`)**
   - 以卫星身份加入 DDS 域。
   - 监听任务与控制指令。
   - 执行 CBBA 包构造与共识逻辑。
   - 将状态信息回传服务器。

### 启动步骤

1. **启动服务器**

   ```bash
   cargo run --example dds_server
   ```

   程序会等待约 5 秒，期间可让多个 Agent 加入。

2. **启动 Agent（多终端）**

   ```bash
   cargo run --example dds_agent -- 1
   cargo run --example dds_agent -- 2
   # 依需求继续启动更多实例
   ```

   PowerShell 用户可使用 `start` 命令在新窗口中运行：

   ```powershell
   start cargo run --example dds_agent -- 1
   start cargo run --example dds_agent -- 2
   ```

### Ex3：DDS Ping/Pong 冒烟测试

`examples/ex3_*` 目录提供最小化 DDS 测试：

- `ex3_dds_runner` 会派生 N 个 Agent 和 1 个 Syncer 子进程，并传入统一的 `--domain`、`--agents`、`--interval` 参数。
- 默认日志过滤器会屏蔽 RustDDS 的 UDP IPv6 噪声，如需调试可自定义 `RUST_LOG`。
- 借助项目根目录的 `Justfile` 管理构建与运行顺序。

使用 Just 命令：

```bash
just run-ex3-runner domain=0 agents=3 interval=3
```

或直接运行 Cargo：

```bash
RUST_LOG=info,rustdds=warn cargo run --release --example ex3_dds_runner -- --domain 0
```

### 通信架构速览

- **Tasks**：Server 通过 `NewTask` 主题向 Agent 发布任务。
- **Control**：Server 使用 `ServerControl` 主题广播开始/停止指令。
- **Consensus**：Agent 之间通过 `ConsensusMessage` 主题交换竞价信息。
- **Sync**：Agent 将 `SyncMessage` 发送给 Server 报告状态。
- **State**：Agent 周期性向 Server 发送 `AgentState` 以支持可视化。

在异步实现中，Agent 使用 `tokio` 运行时，通过通道解耦 DDS 读写器与业务逻辑，确保网络 IO 与主循环均保持非阻塞。

---

更多示例与深入文档请参阅 `examples/`、`docs/` 目录，欢迎通过 issue 反馈需求。
