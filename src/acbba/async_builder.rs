use super::data_types::{
    WinnersMessage, WinningMessage, Player, TaskInfo, WinnerId, TaskId, Score, ObWindow, AgentId,
};
use chashmap::CHashMap;
use parking_lot::RwLock;
use rustdds::{Duration, Timestamp};
use std::collections::{HashMap, HashSet};
use std::{sync::Arc, time};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

 // 更新cognition_core 的操作已 inline，移除了单独的方法定义

#[derive(Debug, Clone)]
pub struct AsyncBuilder {
    pub agent_id: AgentId,
    cognition: Arc<CHashMap<TaskId, WinningMessage>>,
    dynamic_task_pool: Option<HashMap<TaskId, TaskInfo>>,
    selected_set: HashSet<TaskId>,
    selected_list: Vec<TaskId>,
    selected_windows: Vec<ObWindow>,
    outgoing_winners: Option<WinnersMessage>,
}

impl AsyncBuilder {
    pub fn new(
        agent_id: AgentId,
        cognition: Arc<CHashMap<TaskId, WinningMessage>>,
    ) -> AsyncBuilder {
        AsyncBuilder {
            agent_id,
            cognition,
            dynamic_task_pool: None,
            selected_set: HashSet::new(),
            selected_list: Vec::new(),
            selected_windows: Vec::new(),
            outgoing_winners: None,
        }
    }
    pub fn reset(&mut self) {
        self.selected_set = HashSet::new();
        self.selected_list = Vec::new();
        self.selected_windows = Vec::new();
        self.outgoing_winners = None;
    }

    pub fn add_task(
        &mut self,
        task_id: TaskId,
        ob_window: ObWindow,
    ) {
        self.selected_set.insert(task_id);
        self.selected_list.push(task_id);
        self.selected_windows.push(ob_window);
    }

    pub fn append_outgoing_winner(&mut self, winner: WinningMessage) {
        match &mut self.outgoing_winners {
            // 如果当前存在 bundle，则在 bundle 尾部增添一条 winner
            Some(outgoing) => {
                outgoing.winners.push(winner);
            }
            // 如果当前不存在 bundle，则新建一个带一条 winner 的消息
            None => {
                self.outgoing_winners = Some(WinnersMessage {
                    sender_id: self.agent_id.clone(),
                    winners: vec![winner],
                });
            }
        }
    }

    fn claim_task(&mut self, task: TaskInfo, timestamp: Timestamp) {
        let new_winner = WinningMessage {
            task_id: task.id,
            sender_id: self.agent_id.clone(),
            winner_id: WinnerId::Sure(self.agent_id.clone()),
            score: task.score,
            timestamp,
        };
        self.append_outgoing_winner(new_winner);
        self.cognition.insert(task.id, new_winner);
        self.add_task(task.id, task.ob_window);
    }


    pub fn pop_one_task(
        &self,
        candidate_task_ids: &HashSet<TaskId>,
    ) -> Option<TaskInfo> {
        let mut best_task: Option<TaskInfo> = None;
        let mut max_score = Score(0);

        // guard: require a task pool; if absent, nothing to pop
        let task_pool = self.dynamic_task_pool.as_ref()?;

        for task_id in candidate_task_ids.iter() {

            // guard: require the task to exist in the pool (concise)
            let Some(task_info) = task_pool.get(task_id).copied() else { continue };

            // guard: skip if time-window conflicts with already selected tasks
            if task_info.ob_window.overlaps_any(&self.selected_windows) {
                continue;
            }

            let score = task_info.score;

            // guard: only consider tasks with utility strictly greater than current max
            if score <= max_score {
                continue;
            }

            // 需要判断该任务在cognition_core中是否存在。
            // 先处理不存在（None）情况，再处理存在且优于当前认知的情况，保持逻辑清晰。
            match self.cognition.get(task_id).map(|p| *p) {
                // 如果不存在，说明还很新，直接记录
                None => {
                    best_task = Some(task_info);
                    max_score = score;
                }
                // 如果存在，则仅在冒泡效能优于或等于认知中的效能时记录
                Some(winner) => {
                    // If our candidate utility is strictly greater, we can take it.
                    if score > winner.score {
                        best_task = Some(task_info);
                        max_score = score;
                    } else {
                        // If utilities are effectively equal, apply deterministic tie-break:
                        // - If the recorded winner is a concrete agent, we only take the task
                        //   if our `self_guid` is smaller (so we win ties deterministically).
                        // - If the recorded winner is `None` or `Renew`, we may take the task.
                        if score == winner.score {
                            match winner.winner_id {
                                WinnerId::Sure(winner_id) => {
                                    if self.agent_id < winner_id {
                                        best_task = Some(task_info);
                                        max_score = score;
                                    }
                                }
                                _ => {
                                    // Non-`Sure` winner found during tie-break — treat as an unexpected
                                    // situation and panic so it is noticed during testing.
                                    panic!(
                                        "Unexpected non-Sure winner during tie for task {:?}: {:?}",
                                        task_id, winner.winner_id
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        best_task
    }

    pub fn building(&mut self, timestamp: Timestamp) {
        // 初始化一个候选任务集合 candidate_task_ids
        let mut candidate_task_ids: HashSet<TaskId> = if let Some(task_pool) = &self.dynamic_task_pool {
            task_pool.keys().cloned().collect()
        } else {
            HashSet::new()
        };

        let prev_selected_set = self.selected_set.clone();

        self.reset();
        loop {
                // 冒泡下一个最好的任务 — 使用 `let ... else` 减少嵌套
            let Some(task) = self.pop_one_task(&candidate_task_ids) else { break; };
            candidate_task_ids.remove(&task.id);

            // 如果冒泡成功，直接使用 `task` 的字段（inline）
            // 从 cognition 中一次性匹配出具体 `Winner::Sure`，直接绑定 id 与 score，减少嵌套
            let Some(WinningMessage { winner_id: WinnerId::Sure(prev_winner_id), score: prev_winner_score, .. }) = self.cognition.get(&task.id).map(|p| *p) else {
                // 如果不存在已有认知或不是具体节点，则直接根据冒泡更新
                self.claim_task(task, timestamp);
                continue;
            };

            // 确认记录的是具体节点编号（Winner::Sure），继续比较
            // 如果记录的本就是自己
            if self.agent_id == prev_winner_id {
                // 当前select_set，select_list中，加入该任务
                self.add_task(task.id, task.ob_window);
                // 如果记录的效能，与冒泡的效能不一致，则需更新;如果一致，什么都不用做
                if prev_winner_score != task.score {
                    let new_winner = WinningMessage {
                        task_id: task.id,
                        sender_id: self.agent_id.clone(),
                        winner_id: WinnerId::Sure(self.agent_id),
                        score: task.score,
                        timestamp,
                    };
                    self.append_outgoing_winner(new_winner);
                    self.cognition.insert(task.id, new_winner);
                }
                continue;
            }

            // 如果冒泡的效能更大，则更新认知、发送
            if task.score > prev_winner_score {
                self.claim_task(task, timestamp);
                continue
            }

            // 如果效能一样，则仅当 当前节点编号更小 条件时，更新认知、发送
            if task.score == prev_winner_score {
                // prev_winner.agent_id 已知为 Winner::Sure(old_winner_uuid)
                if self.agent_id < prev_winner_id {
                    self.claim_task(task, timestamp);
                }
            }
        }

        // 对于存在于旧的选定任务集合，但不存在于新的选定任务集合的任务，需要额外注意。也就是说，认知里仍然归自己做，但本次选定后没做，这时要去除
        for prev_task_id in prev_selected_set.into_iter() {
            if !self.selected_set.contains(&prev_task_id) {
                // 如果认知里，任务归自己做 —— 一次性匹配出具体 Winner::Sure
                if let Some(WinningMessage { winner_id: WinnerId::Sure(prev_winner_uuid), .. }) = self.cognition.get(&prev_task_id).map(|p| *p) {
                    if prev_winner_uuid == self.agent_id {
                        let new_piece = WinningMessage {
                            task_id: prev_task_id,
                            sender_id: self.agent_id.clone(),
                            winner_id: WinnerId::None,
                            score: Score(0),
                            timestamp,
                        };
                        self.append_outgoing_winner(new_piece);
                        self.cognition.insert(prev_task_id, new_piece);
                    }
                }
            }
        }
    }

    // 需要有第一次building的过程，不基于消息响应，而是初始时即自动建立
    pub async fn first_time_building(
        dp_guid_prefix: AgentId,
        player_type: Player,
        h_to_b_tx: UnboundedSender<Timestamp>,
        lock_time_offset: Arc<RwLock<Option<Duration>>>,
    ) {
        let get_timestamp_wrt_server = |clk_offset_opt: Option<Duration>| -> Option<Timestamp> {
            match clk_offset_opt {
                /*如果时钟偏差存在，则可以正常处理。读取时钟偏差*/
                Some(clock_offset) => {
                    println!("{0:?} clock offset is {1:?}", dp_guid_prefix, clock_offset);
                    Some(Timestamp::now() - clock_offset)
                }
                /*如果时钟偏差存在，分2种情况*/
                None => {
                    if let Player::Client = player_type {
                        // 如果1）当前节点是Client，说明时钟还没对准，则不能处理信息，返回None
                        println!("{0:? }clock offset has not been set", dp_guid_prefix);
                        None
                    } else {
                        // 如果时钟偏差不存在：2）当前节点是Server，无需对准时钟，所以正常处理
                        Some(Timestamp::now())
                    }
                }
            }
        };
        'first_build: loop {
            let clock_offset = {
                let r = lock_time_offset.read();
                *r
            };
            /* 通过判断修正后的时间，只有时间有修正后，才building，否则短暂休息后反复重试 */
            let time_stamp = get_timestamp_wrt_server(clock_offset);
            if let Some(timestamp_wrt_s) = time_stamp {
                if let Err(e) = h_to_b_tx.send(timestamp_wrt_s) {
                    eprintln!("first time building send error: {e}");
                }
                println!("first time built");
                break 'first_build;
            } else {
                let delta_time = time::Duration::from_secs(3);
                tokio::time::sleep(delta_time).await;
            }
        }
    }
    // 定期，向外发送自己的认知，用于防止正常通信的丢包
    pub fn periodic_rebroad_local_cognition_core(
        &self,
        sender_tx: &UnboundedSender<WinnersMessage>,
    ) {
        let mut pieces: Vec<WinningMessage> = Vec::new();
        //
        // self.bundle_to_send = Some(new_bundle);
        let cognition_core = (*self.cognition).clone();
        for (_, piece) in cognition_core.into_iter() {
            pieces.push(piece);
        }

        let bundle_deliver = WinnersMessage {
            sender_id: self.agent_id.clone(),
            winners: pieces,
        };
        if let Err(e) = sender_tx.send(bundle_deliver) {
            eprintln!("periodic rebroad send error: {e}");
        }
        // 打印当前任务列表
        let mut b = self.selected_list.clone();
        b.sort_unstable();
        println!("");
        println!(
            "{:?}'s task list is \t{:?}",
            &self.agent_id,
            b,
            // self.select_list_as_ref(),
        )
    }

    // 赋值任务信息
    // pub fn assign_task_info(&mut self, task_info_updation: HashMap<String, TaskInfo>) {
    //     self.dynamic_task_pool = Some(task_info_updation);
    // }

    // 更新任务信息
    pub fn update_task_info(&mut self, task_info_updation: HashMap<TaskId, Option<TaskInfo>>) {
        let task_pool_optional = self.dynamic_task_pool.take();
        // 读取现有的任务信息池
        let mut task_pool: HashMap<TaskId, TaskInfo> = {
            if let Some(task_pool) = task_pool_optional {
                task_pool
            } else {
                HashMap::new()
            }
        };
        for (task_id, task_info_optional) in task_info_updation.into_iter() {
            match task_info_optional {
                // 如果更新的信息有内容
                Some(task_info) => {
                    let _ = task_pool.insert(task_id, task_info);
                }
                // 如果更新的信息是None，则从dynamic_task_pool中删除对应任务
                None => {
                    let _ = task_pool.remove(&task_id);
                }
            }
        }
        // 如果任务池不为空，则填写Some，否则填写None
        if !task_pool.is_empty() {
            self.dynamic_task_pool = Some(task_pool);
        }
    }
}

pub async fn loop_builder(
    dp_guid_prefix: AgentId,
    mut h_to_b_rx: UnboundedReceiver<Timestamp>,
    mut t_to_b_rx: UnboundedReceiver<bool>,
    bh_to_s_tx: UnboundedSender<WinnersMessage>,
    mut e_to_b_rx: UnboundedReceiver<HashMap<TaskId, Option<TaskInfo>>>,
    cognition_core: Arc<CHashMap<TaskId, WinningMessage>>,
) {
    println!("{:?} async builder", dp_guid_prefix);

    // 构造一个builder
    let mut tasking_builder = Box::new(AsyncBuilder::new(
        dp_guid_prefix,
        cognition_core,
    ));

    loop {
        tokio::select! {
            Some(timestamp) = h_to_b_rx.recv() => {
                // 重新building一次本地选定任务，更新select_set,select_list, bundle_to_send
                tasking_builder.building(timestamp);
                // 如果bundle_to_send不为None，则发送
                if let Some(bundle_deliver) = tasking_builder.outgoing_winners.take() {
                    if let Err(e) = bh_to_s_tx.send(bundle_deliver) {
                        eprintln!("builder send bundle error: {e}");
                    }
                }
            }
            Some(_) = t_to_b_rx.recv() => {
                tasking_builder.periodic_rebroad_local_cognition_core(&bh_to_s_tx);
            }
            Some(task_info_updation) = e_to_b_rx.recv() => {
                tasking_builder.update_task_info(task_info_updation);
            }
            else => break,
        }
    }
}
// pub trait RangeIntersect {
//     fn intersected_with(&self, other: &TaskObWindowType) -> bool;
// }
// pub type TaskObWindowType = Range<DDSTimestamp>;

// impl RangeIntersect for TaskObWindowType {
//     fn intersected_with(&self, other: &TaskObWindowType) -> bool {
//         if self.start >= other.end || self.end <= other.start {
//             false
//         } else {
//             true
//         }
//     }
// }
