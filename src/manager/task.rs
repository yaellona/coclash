//! 任务层：异步任务发起与结果回灌，自成一体。
//! `Manager` 只做一行转发，不感知任何任务细节。
//!
//! # 并发纪律
//!
//! - 全项目共享状态只有一把锁：`Mutex<AppState>`（`TaskRunner.state`）
//! - **绝不跨 `await` 持有锁**：每个任务分三段——短临界区（预置）→
//!   `await`（无锁）→ 短临界区（回灌）
//! - HTTP 请求、轮询等慢操作一律在锁外；config 序列化 + 写盘（<1ms）允许锁内
//! - 锁内只做微秒级操作，毒锁由 `Manager::state_lock` 统一恢复
use crate::manager::state::Node;
use crate::core::mihomo::{self, ApiClient};
use crate::error::Error;
use crate::manager::state::AppState;
use crate::operation_log::LogType;
use crate::settings::Settings;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// 任务发起器：持有 api/设置/配置路径与共享状态，`Manager` 只做转发
pub struct TaskRunner {
    api: Arc<ApiClient>,
    settings: Settings,
    config_path: PathBuf,
    state: Arc<Mutex<AppState>>,
}

impl TaskRunner {
    pub fn new(
        settings: &Settings,
        config_path: PathBuf,
        group_name: &str,
        state: Arc<Mutex<AppState>>,
    ) -> Result<Self, Error> {
        let api = Arc::new(ApiClient::new(settings, group_name)?);
        Ok(Self {
            api,
            settings: settings.clone(),
            config_path,
            state,
        })
    }

    fn spawn(&self, fut: impl std::future::Future<Output = ()> + Send + 'static) {
        tokio::spawn(fut);
    }

    /// 拉取节点列表并回灌（任务内部与成功续发共用）
    pub fn load_nodes(&self) {
        let api = self.api.clone();
        let state = self.state.clone();
        self.spawn(async move {
            load_nodes_impl(&api, &state).await;
        });
    }

    /// 测速：预置 waiting 状态 → 异步测速 → 回灌结果
    pub fn start_delay_test(&self) {
        let api = self.api.clone();
        let state = self.state.clone();
        self.spawn(async move {
            {
                // 预置（守卫：已在测速则拒绝）
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                if st.is_test_delay {
                    st.logs.add_log(LogType::Warn, "已经在测速了!".into());
                    return;
                }
                st.is_test_delay = true;
                for node in &mut st.nodes {
                    node.speed = "wait".to_string();
                }
            }
            let result = api.fetch_delays().await;
            // 回灌
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.is_test_delay = false;
            match result {
                Ok(map) => {
                    for node in &mut st.nodes {
                        node.speed = match map.get(&node.name) {
                            Some(&d) => format!("{d}ms"),
                            None => "-".to_string(),
                        };
                    }
                    st.logs.add_log(LogType::Info, "测速完成".into());
                }
                Err(e) => st.logs.add_log(LogType::Error, e.to_string()),
            }
        });
    }

    /// 切换节点：乐观预置 active_node → 异步切换 → 回灌日志
    /// 守卫：切换进行中拒绝新任务（连按 Enter 不产生交错请求）
    pub fn switch_node(&self, index: usize) {
        let api = self.api.clone();
        let state = self.state.clone();
        self.spawn(async move {
            let name = {
                // 预置
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                if st.is_switching_node {
                    st.logs.add_log(LogType::Warn, "正在切换节点，请稍候".into());
                    return;
                }
                let Some(node) = st.nodes.get(index) else {
                    return;
                };
                let name = node.name.clone();
                st.is_switching_node = true;
                st.active_node = Some(index);
                name
            };
            let result = api.switch_node(&name).await;
            // 回灌
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.is_switching_node = false;
            match result {
                Ok(()) => st.logs.add_log(LogType::Info, format!("切换节点：{name}")),
                Err(e) => st.logs.add_log(LogType::Error, e.to_string()),
            }
        });
    }

    /// 重载配置，成功后续发拉取节点
    pub fn reload_config(&self) {
        let api = self.api.clone();
        let path = self.config_path.clone();
        let state = self.state.clone();
        let retry = self.settings.provider_retry.max(1);
        let interval = self.settings.provider_retry_interval();
        self.spawn(async move {
            reload_config_impl(&api, &state, &path, retry, interval).await;
        });
    }

    /// 添加订阅：锁内按「订阅{n}」命名 → 锁内插入 → 锁外写盘 → 重载
    pub fn insert_sub(&self, url: String) {
        let api = self.api.clone();
        let state = self.state.clone();
        let config_path = self.config_path.clone();
        let retry = self.settings.provider_retry.max(1);
        let interval = self.settings.provider_retry_interval();
        self.spawn(async move {
            let name = {
                let st = state.lock().unwrap_or_else(|e| e.into_inner());
                let n = st
                    .config
                    .proxy_providers
                    .as_ref()
                    .map(|p| p.len())
                    .unwrap_or(0)
                    + 1;
                format!("订阅{n}")
            };
            // 回灌：锁内纯内存插入
            {
                let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
                st.config.insert_sub(url, name.clone());
            }
            // 锁外写盘 + 续发重载
            match save_config(&state, &config_path) {
                Ok(()) => {
                    state
                        .lock()
                        .unwrap_or_else(|e| e.into_inner())
                        .logs
                        .add_log(LogType::Info, format!("插入订阅：{name}"));
                    reload_config_impl(&api, &state, &config_path, retry, interval).await;
                }
                Err(e) => state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .logs
                    .add_log(LogType::Error, e.to_string()),
            }
        });
    }

    /// 启动后就绪探测：轮询控制端口直到就绪或超时，就绪后拉取节点
    pub fn wait_for_start(&self) {
        let ctrl_addr = self.settings.mihomo_ctrl_addr.clone();
        let attempts = self.settings.provider_retry.max(1);
        let interval = self.settings.provider_retry_interval();
        let api = self.api.clone();
        let state = self.state.clone();
        self.spawn(async move {
            let mut ready = false;
            for _ in 0..attempts {
                if mihomo::process::ctrl_addr_up_async(&ctrl_addr).await {
                    ready = true;
                    break;
                }
                tokio::time::sleep(interval).await;
            }
            if !ready {
                ready = mihomo::process::ctrl_addr_up_async(&ctrl_addr).await;
            }
            if ready {
                state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .logs
                    .add_log(LogType::Info, "mihomo 已就绪，正在拉取节点".into());
                load_nodes_impl(&api, &state).await;
            } else {
                state
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .logs
                    .add_log(
                        LogType::Warn,
                        "进程已启动但端口未就绪（启动可能较慢或失败），可按 s 停止后重试".into(),
                    );
            }
        });
    }

    /// config 落盘：锁内序列化 → 锁外写盘（设置/代理窗口修改后的统一入口）
    pub(crate) fn save_config(&self) -> Result<(), Error> {
        save_config(&self.state, &self.config_path)
    }
}

// ===== 任务实现（与 TaskRunner 方法一一对应，可被续发复用） =====

/// 拉取节点列表并回灌到共享状态
async fn load_nodes_impl(api: &ApiClient, state: &Arc<Mutex<AppState>>) {
    match api.get_proxy().await {
        Ok(proxy) => {
            let mut st = state.lock().unwrap_or_else(|e| e.into_inner());
            st.active_node = proxy.all.iter().position(|n| *n == proxy.now);
            st.nodes = proxy.all.into_iter().map(Node::new).collect();
            // 列表变化后只收敛越界的光标，不跳回第一行（后台刷新不得打断用户浏览位置）
            let max = st.nodes.len().saturating_sub(1);
            st.select = st.select.min(max);
            st.logs.add_log(LogType::Info, "更新代理信息".into());
        }
        Err(e) => state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .logs
            .add_log(LogType::Error, e.to_string()),
    }
}

/// 重载配置并刷新显示列表。provider 由 mihomo 后台异步拉取，
/// 无论 reload 成败都先刷新一次，再轮询直到节点不再只有 DIRECT 兜底。
async fn reload_config_impl(
    api: &ApiClient,
    state: &Arc<Mutex<AppState>>,
    path: &Path,
    retry: u32,
    interval: Duration,
) {
    match api.reload_config(path).await {
        Ok(()) => {
            state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .logs
                .add_log(LogType::Info, "重置配置成功".into());
        }
        Err(e) => {
            state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .logs
                .add_log(LogType::Error, e.to_string());
        }
    }
    for _ in 0..retry {
        load_nodes_impl(api, state).await;
        let populated = state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .nodes
            .len()
            > 1;
        if populated {
            break;
        }
        tokio::time::sleep(interval).await;
    }
}

/// 锁内克隆（短临界区）→ 锁外序列化+写盘（统一收敛到 `MihomoConfig::write_to_path`）
fn save_config(state: &Arc<Mutex<AppState>>, config_path: &Path) -> Result<(), Error> {
    let config = state.lock().unwrap_or_else(|e| e.into_inner()).config.clone();
    config.write_to_path(&PathBuf::from(config_path))
}
