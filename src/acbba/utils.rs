use super::data_types::{TaskInfo, TaskId, Score, ObWindow};
use rand::Rng;

pub fn generate_random_task_info(task_id: TaskId) -> TaskInfo {
    // choose the ranges here and pass them into ObWindow::random_with
    let ob_window = ObWindow::random_with(100..1001, 20..61);
    let mut rng = rand::rng();
    let utility_val = rng.random_range(1..101) as u32;
    let utility = Score(utility_val);

    TaskInfo { id: task_id, ob_window, score: utility }
}
