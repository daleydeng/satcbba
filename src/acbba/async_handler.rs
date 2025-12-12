use super::data_types::{WinnersMessage, WinningMessage, Player, WinnerId, TaskId, AgentId};
use chashmap::CHashMap;
use parking_lot::RwLock;
use rustdds::{Duration, Timestamp};
use std::{collections::HashSet, sync::Arc};
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

#[derive(Debug, Clone)]
pub struct AsyncHandler {
    dp_guid_prefix: AgentId,
    cognition_core: Arc<CHashMap<TaskId, WinningMessage>>,
    // select_set: HashSet<String>,
    // select_list: Vec<String>,
    // select_timeline: Vec<TaskObWindowType>,
    bundle_to_send: Option<WinnersMessage>,
    rule_sets_list: Vec<HashSet<i32>>,
    builder_trigger: Option<bool>,
}

pub struct ResponseStrategyType {
    is_update_all: bool,
    is_rebroadcast_updated: bool,
    is_update_time_only: bool,
    is_rebroadcast_leave: bool,
    is_reset: bool,
    is_rebroadcast_empty: bool,
}
impl ResponseStrategyType {
    /* 读取 */
    pub fn is_update_all(&self) -> bool {
        self.is_update_all
    }
    pub fn is_rebroadcast_updated(&self) -> bool {
        self.is_rebroadcast_updated
    }
    pub fn is_update_time_only(&self) -> bool {
        self.is_update_time_only
    }
    pub fn is_rebroadcast_leave(&self) -> bool {
        self.is_rebroadcast_leave
    }
    pub fn is_reset(&self) -> bool {
        self.is_reset
    }
    pub fn is_rebroadcast_empty(&self) -> bool {
        self.is_rebroadcast_empty
    }
    /* 设置 */
    pub fn new() -> ResponseStrategyType {
        ResponseStrategyType {
            is_update_all: false,
            is_rebroadcast_updated: false,
            is_update_time_only: false,
            is_rebroadcast_leave: false,
            is_reset: false,
            is_rebroadcast_empty: false,
        }
    }
    pub fn set_update_all(&mut self) {
        self.is_update_all = true;
    }
    pub fn set_rebroadcast_updated(&mut self) {
        self.is_rebroadcast_updated = true;
    }
    pub fn set_update_time_only(&mut self) {
        self.is_update_time_only = true;
    }
    pub fn set_rebroadcast_leave(&mut self) {
        self.is_rebroadcast_leave = true;
    }
    pub fn set_reset(&mut self) {
        self.is_reset = true;
    }
    pub fn set_rebroadcast_empty(&mut self) {
        self.is_rebroadcast_empty = true;
    }
}
impl AsyncHandler {
    pub fn new(
        dp_guid_prefix: AgentId,
        cognition_core: Arc<CHashMap<TaskId, WinningMessage>>,
    ) -> AsyncHandler {
        let rule_sets_list = {
            let arr_0 = vec![
                1, 3, 6, 8, 9, 10, 11, 12, 13, 16, 17, 25, 28, 30, 32, 34, 35, 36, 38, 40, 41, 42,
                45, 47,
            ];
            let arr_1 = vec![2, 31];
            let arr_2 = vec![
                4, 14, 15, 19, 20, 23, 24, 27, 29, 33, 39, 43, 44, 46, 48, 49,
            ];
            let arr_3 = vec![5, 18, 26, 37];
            let arr_4 = vec![21];
            let arr_5 = vec![7, 22];
            let arr_6 = vec![50];
            let arr_list = vec![arr_0, arr_1, arr_2, arr_3, arr_4, arr_5, arr_6];
            let mut rule_sets_list: Vec<HashSet<i32>> = Vec::new();
            for i in 0..7 {
                let mut rule_set: HashSet<i32> = HashSet::new();
                let arr = &arr_list[i];
                for i in arr.iter() {
                    rule_set.insert(*i);
                }
                rule_sets_list.push(rule_set);
            }
            rule_sets_list
        };
        // println!("rule_sets_list is : {:?}", rule_sets_list);

        AsyncHandler {
            dp_guid_prefix: dp_guid_prefix,
            cognition_core,
            // select_set: HashSet::new(),
            // select_list: Vec::new(),
            bundle_to_send: None,
            rule_sets_list: rule_sets_list,
            builder_trigger: None,
        }
    }
    // 读取
    // pub fn get_select_set(&self) -> HashSet<String> {
    //     self.select_set.clone()
    // }
    // pub fn get_select_list(&self) -> Vec<String> {
    //     self.select_list.clone()
    // }
    // pub fn get_cognition_core(&self) -> Arc<CHashMap<String, Piece>> {
    //     self.cognition_core.clone()
    // }

    //
}
impl AsyncHandler {
    // 常量，允许的时间同步误差
    pub const CLOCK_OFFSET_ALLOWED: Duration = Duration::from_millis(300);
}
impl AsyncHandler {
    // 设置builder_trigger的方法
    pub fn trigger_set(&mut self) {
        self.builder_trigger = Some(true);
    }
    pub fn trigger_clear(&mut self) {
        self.builder_trigger = None;
    }
    // 读取builder_trigger的方法
    pub fn trigger_take(&mut self) -> Option<bool> {
        self.builder_trigger.take()
    }

    pub fn rebroad_bundle_take(&mut self) -> Option<WinnersMessage> {
        self.bundle_to_send.take()
    }
    pub fn rebroad_bundle_add(&mut self, piece: WinningMessage) {
        match &mut self.bundle_to_send {
            // 如果当前存在bundle，则在bundle尾部增添一条piece
            Some(bundle) => {
                bundle.winners.push(piece);
            }
            // 如果当前不存在bundle，则新建一个bundle带一条piece
            None => {
                self.bundle_to_send = Some(WinnersMessage {
                    sender_id: self.dp_guid_prefix.clone(),
                    winners: vec![piece],
                });
            }
        }
    }

    // 更新cognition_core的方法
    pub fn update_cognition_piece(&self, task_id: TaskId, new: WinningMessage) {
        self.cognition_core.insert(task_id, new);
    }
    pub fn update_cognition_time(
        &self,
        task_id: TaskId,
        new_timestamp: Timestamp,
    ) -> Option<WinningMessage> {
        let one_optional: Option<WinningMessage> = self.cognition_core.get(&task_id).map(|p| *p);
        match one_optional {
                Some(mut old) => {
                old.timestamp = new_timestamp;
                self.cognition_core.insert(task_id, old);
                Some(old)
            }
            None => None,
        }
    }
    pub fn update_cognition_empty(&self, task_id: TaskId, new_timestamp: Timestamp) -> WinningMessage {
        let empty_cognition_piece = WinningMessage {
            task_id: task_id,
            sender_id: self.dp_guid_prefix.clone(),
            winner_id: WinnerId::None,
            score: super::data_types::Score(0),
            timestamp: new_timestamp,
        };
        self.cognition_core
            .insert(task_id, empty_cognition_piece);
        empty_cognition_piece
    }
    pub fn reset_cognition_empty(&self, task_id: TaskId) {
        let one_optional: Option<WinningMessage> = self.cognition_core.get(&task_id).map(|p| *p);
        match one_optional {
                Some(mut old) => {
                old.winner_id = WinnerId::None;
                    old.score = super::data_types::Score(0);
                self.cognition_core.insert(task_id, old);
            }
            None => {}
        }
    }
}

impl AsyncHandler {
    // 处理一次handle的
    pub fn handle_once(
        &mut self,
        bundle: WinnersMessage,
        new_timestamp: Timestamp,
        builder_tx: &UnboundedSender<Timestamp>,
        sender_tx: &UnboundedSender<WinnersMessage>,
    ) -> () {
        // 首先，清空待发送 bundle_to_send，清除builder_trigger
        self.rebroad_bundle_take();
        self.trigger_clear();
        // 读取所处理的bundle的源other_uuid，及所有条目pieces
        let other_uuid = bundle.sender_id;
        let pieces = bundle.winners;
        /* 遍历每条piece */

        for other in pieces.into_iter() {
            // 读取该条目对应的task_id
            let task_id = other.task_id.clone();
            // 尝试从本地认知中，得到关于该task_id的一条认知，结果是one_optional
            // 由于chashmap是带锁的，所以要尽量释放锁，用scope

            let one_optional: Option<WinningMessage> = self.cognition_core.get(&task_id).map(|p| *p);

            // 根据one_optional的有无，判断读取的本地认知是否存在
            match one_optional {
                // 如果本地已有关于该task_id的认知，读出为one
                Some(one) => {
                    // 通过check_update_and_response，归类得到应答规则的编号

                    let response_rule = AsyncHandler::check_update_and_response(
                        &self.dp_guid_prefix,
                        &one,
                        &other_uuid,
                        &other,
                    );

                    match response_rule {
                        Some(rule) => {
                            // 根据规则编号，获取下一步动作策略集合 action_strategies

                            let action_strategies = self.response_strategies(rule);

                            if action_strategies.is_reset() {
                                self.reset_cognition_empty(task_id.clone());
                            }

                            if action_strategies.is_update_all() {
                                self.update_cognition_piece(task_id.clone(), other.clone());
                                self.trigger_set();
                            }

                            if action_strategies.is_update_time_only() {
                                if let Some(new_piece) =
                                    self.update_cognition_time(task_id.clone(), new_timestamp)
                                {
                                    self.rebroad_bundle_add(new_piece);
                                }
                            }

                            if action_strategies.is_rebroadcast_updated() {
                                self.rebroad_bundle_add(other);
                            }

                            if action_strategies.is_rebroadcast_leave() {
                                self.rebroad_bundle_add(one);
                            }

                            if action_strategies.is_rebroadcast_empty() {
                                let empty_piece =
                                    self.update_cognition_empty(task_id, new_timestamp);
                                self.rebroad_bundle_add(empty_piece);
                            }
                        }
                        None => {
                            println!("不应出现的规则编号，请检查");
                        }
                    }
                }
                // 如果本地之前没有，则将other存入库中；同时，bundle_to_send添加一条，用于rebroadcast
                None => {
                    self.cognition_core.insert(task_id, other.clone());
                    self.rebroad_bundle_add(other);
                }
            }

            // let winner_score = other.winner_score();
        }
        // println!("handle once #3");

        // 如果待发送区存在,则需要发送给Sender
        if let Some(bundle_deliver) = self.rebroad_bundle_take() {
            if let Err(e) = sender_tx.send(bundle_deliver) {
                eprintln!("handler send bundle error: {e}");
            }
        }
        // 如果需要build更新一次本地决策，则发送给Builder
        if let Some(_) = self.trigger_take() {
            if let Err(e) = builder_tx.send(new_timestamp) {
                eprintln!("handler send timestamp error: {e}");
            }
        }
        // println!("handle once #4");
    }
    pub fn check_update_and_response(
        one_uuid: &AgentId,
        one: &WinningMessage,
        other_uuid: &AgentId,
        other: &WinningMessage,
    ) -> Option<i32> {
        let (x_winner_typed, x_score, x_time) = (&one.winner_id, one.score, &one.timestamp);
        let (y_winner_typed, y_score, y_time) = (&other.winner_id, other.score, &other.timestamp);
        let i = one_uuid;
        let k = other_uuid;
        let approximately_equal = |t1: &Timestamp, t2: &Timestamp| -> bool {
            if (*t1 < *t2 + AsyncHandler::CLOCK_OFFSET_ALLOWED)
                && (*t1 > *t2 - AsyncHandler::CLOCK_OFFSET_ALLOWED)
            {
                true
            } else {
                false
            }
        };
        match (y_winner_typed, x_winner_typed) {
            // y_winner有值，x_winner也有值
            (WinnerId::Sure(y_winner), WinnerId::Sure(x_winner)) => {
                if y_winner == k {
                    if x_winner == i {
                        if y_score > x_score {
                            return Some(1);
                        } else if x_score > y_score {
                            return Some(2);
                        } else {
                            if i > k {
                                return Some(3);
                            } else if i < k {
                                return Some(4);
                            } else {
                                println!("please check #1");
                                return None;
                            }
                        }
                    } else if x_winner == k {
                        if approximately_equal(y_time, x_time) {
                            return Some(5);
                        } else if *y_time > *x_time {
                            return Some(6);
                        } else {
                            return Some(7); //TODO: 对应rule 20，应该做点什么，初步看，本节点应该重置和更新，归类入策略“5”
                        }
                    } else {
                        if y_score > x_score {
                            if approximately_equal(y_time, x_time) {
                                return Some(11);
                            } else if *y_time > *x_time {
                                return Some(12);
                            } else {
                                return Some(13);
                            }
                        } else if x_score > y_score {
                            if approximately_equal(y_time, x_time) {
                                return Some(14);
                            } else if *x_time > *y_time {
                                return Some(15);
                            } else {
                                return Some(16);
                            }
                        } else {
                            if x_winner > y_winner {
                                return Some(17);
                            } else if y_winner > x_winner {
                                return Some(49);
                            } else {
                                println!("please check #2");
                                return None;
                            }
                        }
                    }
                } else if y_winner == i {
                    if x_winner == i {
                        if approximately_equal(y_time, x_time) {
                            return Some(18);
                        } else if *x_time > *y_time {
                            return Some(19);
                        } else {
                            println!("please check #3");
                            // TODO : 一旦这种情况发生，应该做点什么，例如重新规划
                            return Some(20);
                        }
                    } else if x_winner == k {
                        return Some(21);
                    } else {
                        return Some(23);
                    }
                } else {
                    if x_winner == i {
                        if y_score > x_score {
                            return Some(30);
                        } else if x_score > y_score {
                            return Some(31);
                        } else {
                            if i > y_winner {
                                return Some(32);
                            } else if i < y_winner {
                                return Some(33);
                            } else {
                                println!("please check #4");
                                return None;
                            }
                        }
                    } else if x_winner == k {
                        if approximately_equal(y_time, x_time) {
                            return Some(34);
                        } else if *y_time > *x_time {
                            return Some(35);
                        } else {
                            return Some(36);
                        }
                    } else if x_winner == y_winner {
                        if approximately_equal(y_time, x_time) {
                            return Some(37);
                        } else if *y_time > *x_time {
                            return Some(38);
                        } else {
                            return Some(39);
                        }
                    } else {
                        if y_score > x_score {
                            if approximately_equal(y_time, x_time) {
                                return Some(41);
                            } else if *y_time > *x_time {
                                return Some(42);
                            } else {
                                return Some(43);
                            }
                        } else if x_score > y_score {
                            if approximately_equal(y_time, x_time) {
                                return Some(44);
                            } else if *y_time > *x_time {
                                return Some(45);
                            } else {
                                return Some(46);
                            }
                        } else {
                            if x_winner > y_winner {
                                return Some(47);
                            } else if y_winner > x_winner {
                                return Some(48);
                            } else {
                                println!("please check #5");
                                return None;
                            }
                        }
                    }
                }
            }
            // y_winner有值，x_winner为None
            (WinnerId::Sure(y_winner), WinnerId::None) => {
                if y_winner == k {
                    if approximately_equal(y_time, x_time) {
                        return Some(8);
                    } else if *y_time > *x_time {
                        return Some(9);
                    } else {
                        return Some(10);
                    }
                } else if y_winner == i {
                    return Some(22);
                } else {
                    return Some(40);
                }
            }
            // y_winner有值，x_winner为Nenew
            (WinnerId::Sure(y_winner), WinnerId::Renew) => {
                if y_winner == k {
                    if y_score > x_score {
                        if approximately_equal(y_time, x_time) {
                            return Some(11);
                        } else if *y_time > *x_time {
                            return Some(12);
                        } else {
                            return Some(13);
                        }
                    } else if x_score > y_score {
                        if approximately_equal(y_time, x_time) {
                            return Some(14);
                        } else if *x_time > *y_time {
                            return Some(15);
                        } else {
                            return Some(16);
                        }
                    } else {
                        println!("please check #6");
                        return None;
                    }
                } else if y_winner == i {
                    return Some(23);
                } else {
                    if y_score > x_score {
                        if approximately_equal(y_time, x_time) {
                            return Some(41);
                        } else if *y_time > *x_time {
                            return Some(42);
                        } else {
                            return Some(43);
                        }
                    } else if x_score > y_score {
                        if approximately_equal(y_time, x_time) {
                            return Some(44);
                        } else if *y_time > *x_time {
                            return Some(45);
                        } else {
                            return Some(46);
                        }
                    } else {
                        println!("please check #7");
                        return None;
                    }
                }
            }
            // y_winner为None，x_winner有值
            (WinnerId::None, WinnerId::Sure(x_winner)) => {
                if x_winner == i {
                    return Some(24);
                } else if x_winner == k {
                    return Some(25);
                } else {
                    if approximately_equal(y_time, x_time) {
                        return Some(27);
                    } else if *y_time > *x_time {
                        return Some(28);
                    } else {
                        return Some(29);
                    }
                }
            }
            // y_winner为None，x_winner为None
            (WinnerId::None, WinnerId::None) => {
                return Some(26);
            }
            // y_winner为None，x_winner为Renew
            (WinnerId::None, WinnerId::Renew) => {
                if approximately_equal(y_time, x_time) {
                    return Some(27);
                } else if *y_time > *x_time {
                    return Some(28);
                } else {
                    return Some(29);
                }
            }
            // y_winner为Renew，x_winner有值
            (WinnerId::Renew, WinnerId::Sure(x_winner)) => {
                if x_winner == k {
                    if *y_time > *x_time + AsyncHandler::CLOCK_OFFSET_ALLOWED {
                        return Some(48);
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
            // y_winner为Renew，x_winner为None
            (WinnerId::Renew, WinnerId::None) => return None,
            // y_winner为Renew，x_winner为Renew
            (WinnerId::Renew, WinnerId::Renew) => return None,
        }
    }
    pub fn response_strategies(&self, rule: i32) -> ResponseStrategyType {
        // println!("response_strategies once #1.611");
        let classified_rule = {
            let mut index: usize = 0;
            // println!("rule = {}", rule);
            let inner_classified_rule = loop {
                if self.rule_sets_list[index].contains(&rule) {
                    break index as i32;
                }
                index += 1;
            };
            inner_classified_rule
        };
        // println!("response_strategies once #1.612");
        let mut action_strategies = ResponseStrategyType::new();
        if classified_rule == 0 {
            action_strategies.set_update_all();
            action_strategies.set_rebroadcast_updated();
        } else if classified_rule == 1 {
            action_strategies.set_update_time_only();
        } else if classified_rule == 2 {
            action_strategies.set_rebroadcast_leave();
        } else if classified_rule == 3 {
        } else if classified_rule == 4 {
            action_strategies.set_reset();
            action_strategies.set_rebroadcast_empty();
        } else if classified_rule == 5 {
            action_strategies.set_rebroadcast_empty();
        } else if classified_rule == 6 {
            action_strategies.set_rebroadcast_updated();
        } else {
        }
        // println!("response_strategies once #1.613");
        action_strategies
    }
}

pub async fn loop_handler(
    dp_guid_prefix: AgentId,
    player_type: Player,
    mut l_to_h_rx: UnboundedReceiver<WinnersMessage>,
    bh_to_s_tx: UnboundedSender<WinnersMessage>,
    h_to_b_tx: UnboundedSender<Timestamp>,
    lock_time_offset: Arc<RwLock<Option<Duration>>>,
    cognition_core: Arc<CHashMap<TaskId, WinningMessage>>,
) {
    println!("{:?} async handler", dp_guid_prefix);

    let get_timestamp_wrt_server = |clk_offset_opt: Option<Duration>| -> Option<Timestamp> {
        match clk_offset_opt {
            /*如果时钟偏差存在，则可以正常处理。读取时钟偏差*/
            Some(clock_offset) => {
                // println!("{0:?} clock offset is {1:?}", dp_guid_prefix, clock_offset);
                Some(Timestamp::now() - clock_offset)
            }
            /*如果时钟偏差存在，分2种情况*/
            None => {
                if let Player::Client = player_type {
                    // 如果1）当前节点是Client，说明时钟还没对准，则不能处理信息，返回None
                    // println!("{0:? }clock offset has not been set", dp_guid_prefix);
                    None
                } else {
                    // 如果时钟偏差不存在：2）当前节点是Server，无需对准时钟，所以正常处理
                    Some(Timestamp::now())
                }
            }
        }
    };
    let mut msg_handler = Box::new(AsyncHandler::new(dp_guid_prefix, cognition_core));

    loop {
        tokio::select! {
            Some(bundle) = l_to_h_rx.recv() => {
                let clock_offset = {
                    let r = lock_time_offset.read();
                    *r
                };
                /* 通过判断修正后的时间，正常处理或不处理 */
                let time_stamp = get_timestamp_wrt_server(clock_offset);

                match time_stamp {
                    // 如果读取正确，正常处理
                    Some(timestamp_wrt_s) => {
                        msg_handler.handle_once(
                            bundle,
                            timestamp_wrt_s,
                            &h_to_b_tx,
                            &bh_to_s_tx,
                        );
                    }
                    // 如果读取错误，则不处理
                    None => {}
                }
            }
            else => break,
        }
    }
}
