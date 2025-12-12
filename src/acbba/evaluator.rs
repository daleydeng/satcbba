use super::data_types::{TaskInfo, UpdateInfo, TaskId, AgentId};
use super::utils::generate_random_task_info;
use std::collections::HashMap;
use tokio::sync::mpsc::{UnboundedReceiver, UnboundedSender};

pub async fn loop_evaluate(
    self_guid: AgentId,
    mut l_to_e_rx: UnboundedReceiver<Vec<UpdateInfo>>,
    e_to_b_tx: UnboundedSender<HashMap<TaskId, Option<TaskInfo>>>,
) {
    println!("{:?} async evaluator", self_guid);

    let mut visited_task: HashMap<TaskId, (f64, f64)> = HashMap::new();

    while let Some(data) = l_to_e_rx.recv().await {
        let mut task_pool: HashMap<TaskId, Option<TaskInfo>> = HashMap::with_capacity(20);
        for update_piece in data.into_iter() {
            match update_piece {
                UpdateInfo::Add((task_id, task_location)) => {
                    // 只有当任务描述发生变化时，再重新增减
                    if visited_task.get(&task_id) != Some(&task_location) {
                        // TODO：需要一个评估算法
                        // 测试用，随机生成; task_id 使用 task_id:
                        let task_info = generate_random_task_info(task_id);

                        task_pool.insert(task_id, Some(task_info));
                        visited_task.insert(task_id, task_location);
                    }
                }
                UpdateInfo::Remove(task_id) => {
                    visited_task.remove(&task_id);
                    task_pool.insert(task_id, None);
                }
            }
        }
        if let Err(e) = e_to_b_tx.send(task_pool) {
            eprintln!("evaluator send error: {e}");
        }
    }
}
