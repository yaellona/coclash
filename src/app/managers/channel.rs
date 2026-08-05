use crate::app::tasks;
use tokio::sync::mpsc;

/// 异步任务通道管理器
#[derive(Debug)]
pub struct TaskChannel {
    pub tx: mpsc::Sender<tasks::AsyncTask>,
    pub rx: mpsc::Receiver<tasks::AsyncTask>,
}
