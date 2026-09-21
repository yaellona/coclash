//! 用户触发的异步任务（均为 `impl Manager` 的命令实现）。
//!
//! 说明：启动后的节点/状态同步统一由心跳（`heartbeat.rs`）完成，
//! 本文件只处理「用户动作」——切换节点、测速、重载配置、添加订阅。
//!
//! # 并发纪律
//!
//! - 全项目共享状态只有一把锁：`Shared.state`（毒锁由 `Shared::lock` 统一恢复）
//! - **绝不跨 `await` 持有锁**：每个任务分三段——短临界区（预置）→
//!   `await`（无锁）→ 短临界区（回灌）
//! - 日志去重交给心跳；本文件的任务每次执行都记日志（用户主动触发，需要反馈）
use super::{Manager, Shared};
use crate::error::Error;
use crate::operation_log::LogType;
use std::sync::Arc;

impl Manager {
    /// 切换节点：乐观预置 active_node → 异步切换 → 回灌日志
    /// 守卫：切换进行中拒绝新任务（连按 Enter 不产生交错请求）
    pub fn switch_node(&self, index: usize) {
        let shared = self.shared().clone();
        tokio::spawn(async move {
            switch_node_impl(&shared, index).await;
        });
    }

    /// 测速：预置 waiting 状态 → 异步测速 → 回灌结果
    pub fn start_delay_test(&self) {
        let shared = self.shared().clone();
        tokio::spawn(async move {
            delay_test_impl(&shared).await;
        });
    }

    /// 重载配置；provider 由 mihomo 后台异步拉取，节点刷新交给心跳（立即触发一次）
    pub fn reload_config(&self) {
        let shared = self.shared().clone();
        tokio::spawn(async move {
            reload_config_impl(&shared).await;
        });
    }

    /// 添加订阅：锁内按「订阅{n}」命名 → 锁内插入 → 锁外写盘 → 重载
    pub fn insert_sub(&self, url: String) {
        let shared = self.shared().clone();
        tokio::spawn(async move {
            insert_sub_impl(&shared, url).await;
        });
    }
}

/// config 落盘：锁内克隆（短临界区）→ 锁外序列化 + 写盘
pub(crate) fn write_config(shared: &Arc<Shared>) -> Result<(), Error> {
    let config = shared.lock().config.clone();
    config.write_to_path(&shared.config_path)
}

async fn switch_node_impl(shared: &Arc<Shared>, index: usize) {
    let name = {
        // 预置
        let mut st = shared.lock();
        if st.mihomo.is_switching_node {
            st.logs
                .add_log(LogType::Warn, "正在切换节点，请稍候".into());
            shared.mark_redraw();
            return;
        }
        let Some(node) = st.mihomo.nodes.get(index) else {
            return;
        };
        let name = node.name.clone();
        st.mihomo.is_switching_node = true;
        st.mihomo.active_node = Some(index);
        name
    };
    shared.mark_redraw();
    let result = shared.api.switch_node(&name).await;
    // 回灌
    let mut st = shared.lock();
    st.mihomo.is_switching_node = false;
    match result {
        Ok(()) => st.logs.add_log(LogType::Info, format!("切换节点：{name}")),
        Err(e) => st.logs.add_log(LogType::Error, e.to_string()),
    }
    drop(st);
    shared.mark_redraw();
}

async fn delay_test_impl(shared: &Arc<Shared>) {
    {
        // 预置（守卫：已在测速则拒绝）
        let mut st = shared.lock();
        if st.mihomo.is_test_delay {
            st.logs.add_log(LogType::Warn, "已经在测速了!".into());
            drop(st);
            shared.mark_redraw();
            return;
        }
        st.mihomo.is_test_delay = true;
        for node in &mut st.mihomo.nodes {
            node.speed = "wait".to_string();
        }
    }
    shared.mark_redraw();
    let result = shared.api.fetch_delays().await;
    // 回灌
    let mut st = shared.lock();
    st.mihomo.is_test_delay = false;
    match result {
        Ok(map) => {
            for node in &mut st.mihomo.nodes {
                node.speed = match map.get(&node.name) {
                    Some(&d) => format!("{d}ms"),
                    None => "-".to_string(),
                };
            }
            st.logs.add_log(LogType::Info, "测速完成".into());
        }
        Err(e) => st.logs.add_log(LogType::Error, e.to_string()),
    }
    drop(st);
    shared.mark_redraw();
}

async fn reload_config_impl(shared: &Arc<Shared>) {
    match shared.api.reload_config(&shared.config_path).await {
        Ok(()) => shared.log(LogType::Info, "重置配置成功"),
        Err(e) => shared.log(LogType::Error, e.to_string()),
    }
    // 立即同步一次，缩短 provider 拉取后的空窗期
    shared.sync_now();
}

async fn insert_sub_impl(shared: &Arc<Shared>, url: String) {
    let name = {
        let st = shared.lock();
        let n = st
            .config
            .proxy_providers
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(0)
            + 1;
        format!("订阅{n}")
    };
    // 锁内纯内存插入
    {
        let mut st = shared.lock();
        st.config.insert_sub(url, name.clone());
    }
    shared.mark_redraw();
    // 锁外写盘 + 续发重载
    match write_config(shared) {
        Ok(()) => {
            shared.log(LogType::Info, format!("插入订阅：{name}"));
            reload_config_impl(shared).await;
        }
        Err(e) => shared.log(LogType::Error, e.to_string()),
    }
}
