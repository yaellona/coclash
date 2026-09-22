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
use crate::core::mihomo::{self, BinarySource, MihomoStatus, embedded};
use crate::error::Error;
use crate::operation_log::LogType;
use std::sync::Arc;
use std::time::Duration;

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

    /// 添加订阅：锁内命名 + 插入（同一临界区）→ 锁外写盘 → 重载
    pub fn insert_sub(&self, url: String) {
        let shared = self.shared().clone();
        tokio::spawn(async move {
            insert_sub_impl(&shared, url).await;
        });
    }
}

/// 启停 mihomo（异步任务）：端口探测、UAC 等待、SIGTERM 轮询都是阻塞 IO，
/// 一律放阻塞线程池，避免卡住 UI；`is_toggling` 守卫拒绝并发触发。
pub(crate) fn toggle_mihomo(shared: &Arc<Shared>) {
    let shared = shared.clone();
    tokio::spawn(async move {
        toggle_mihomo_task(&shared).await;
    });
}

/// config 落盘：写盘串行化（save_lock）→ 锁内克隆 → 锁外序列化 + 写盘。
/// save_lock 保证「克隆 + 写盘」原子：并发保存不会出现旧快照覆盖新快照。
pub(crate) fn write_config(shared: &Arc<Shared>) -> Result<(), Error> {
    let _save_guard = shared.save_lock.lock().unwrap_or_else(|e| e.into_inner());
    let config = shared.lock().config.clone();
    config.write_to_path(&shared.config_path)
}

/// 在阻塞线程池执行落盘，返回结果（join 失败按 IO 错误上报）
async fn write_config_blocking(shared: &Arc<Shared>) -> Result<(), Error> {
    let shared = shared.clone();
    tokio::task::spawn_blocking(move || write_config(&shared))
        .await
        .unwrap_or_else(|e| Err(Error::Process(format!("写盘任务失败: {e}"))))
}

async fn toggle_mihomo_task(shared: &Arc<Shared>) {
    {
        // 守卫：启停是重 IO，拒绝并发（连按 s 不会起两个进程）
        let mut st = shared.lock();
        if st.mihomo.is_toggling {
            st.logs
                .add_log(LogType::Warn, "mihomo 操作进行中，请稍候".into());
            drop(st);
            shared.mark_redraw();
            return;
        }
        st.mihomo.is_toggling = true;
    }
    shared.mark_redraw();

    // 端口探测（阻塞 TCP connect）也放阻塞线程池
    let port_up = {
        let settings = shared.settings.clone();
        tokio::task::spawn_blocking(move || mihomo::is_port_up(&settings))
            .await
            .unwrap_or(false)
    };

    let result: Result<Option<(u32, BinarySource)>, Error> = if port_up {
        let settings = shared.settings.clone();
        tokio::task::spawn_blocking(move || mihomo::stop_mihomo(&settings))
            .await
            .unwrap_or_else(|e| Err(Error::Process(format!("停止任务失败: {e}"))))
            .map(|()| None)
    } else {
        let settings = shared.settings.clone();
        let config_path = shared.config_path.clone();
        let elevate = cfg!(windows);
        tokio::task::spawn_blocking(move || mihomo::start_mihomo(&settings, &config_path, elevate))
            .await
            .unwrap_or_else(|e| Err(Error::Process(format!("启动任务失败: {e}"))))
            .map(Some)
    };

    let started = matches!(result, Ok(Some(_)));
    {
        let mut st = shared.lock();
        st.mihomo.is_toggling = false;
        match &result {
            Ok(None) => {
                st.mihomo.status = MihomoStatus::Stopped;
                st.logs.add_log(LogType::Info, "已停止mihomo".into());
            }
            Ok(Some((pid, source))) => {
                st.mihomo.status = MihomoStatus::Running(*pid);
                st.logs.add_log(
                    LogType::Info,
                    format!(
                        "mihomo 已启动 (PID {pid}, 内嵌 {}: {source})",
                        embedded::VERSION
                    ),
                );
            }
            Err(e) => st.logs.add_log(LogType::Error, e.to_string()),
        }
    }
    shared.mark_redraw();

    // 启动时端口绑定需要时间，延迟同步避免立即探测判离线（留下误导日志）
    let delay = if started {
        Duration::from_millis(800)
    } else {
        Duration::ZERO
    };
    super::heartbeat::sync_now_delayed(shared, delay);
}

async fn switch_node_impl(shared: &Arc<Shared>, index: usize) {
    let (name, prev_active) = {
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
        let prev = st.mihomo.active_node;
        st.mihomo.is_switching_node = true;
        st.mihomo.active_node = Some(index);
        (name, prev)
    };
    shared.mark_redraw();
    let result = shared.api.switch_node(&name).await;
    // 回灌
    let mut st = shared.lock();
    st.mihomo.is_switching_node = false;
    match result {
        Ok(()) => st.logs.add_log(LogType::Info, format!("切换节点：{name}")),
        Err(e) => {
            // 乐观预置失败要回滚高亮，不能一直指着没切成功的节点
            if st.mihomo.active_node == Some(index) {
                st.mihomo.active_node = prev_active;
            }
            st.logs.add_log(LogType::Error, e.to_string());
        }
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
        Err(e) => {
            // 测速失败必须清掉 wait 标记：merge_nodes 会按名字保留旧 speed，
            // 否则节点会永远显示“wait”
            for node in &mut st.mihomo.nodes {
                if node.speed == "wait" {
                    node.speed = "-".to_string();
                }
            }
            st.logs.add_log(LogType::Error, e.to_string());
        }
    }
    drop(st);
    shared.mark_redraw();
}

pub(crate) async fn reload_config_impl(shared: &Arc<Shared>) {
    match shared.api.reload_config(&shared.config_path).await {
        Ok(()) => shared.log(LogType::Info, "重置配置成功"),
        Err(e) => shared.log(LogType::Error, e.to_string()),
    }
    // 立即同步一次，缩短 provider 拉取后的空窗期
    shared.sync_now();
}

async fn insert_sub_impl(shared: &Arc<Shared>, url: String) {
    let name = {
        // 命名与插入必须在同一临界区：并发插入各自算 len()+1 会撞名，
        // 且 insert_sub 内部重名改写后，日志必须用返回的真实名字
        let mut st = shared.lock();
        let n = st
            .config
            .proxy_providers
            .as_ref()
            .map(|p| p.len())
            .unwrap_or(0)
            + 1;
        let name = st.config.insert_sub(url, format!("订阅{n}"));
        drop(st);
        shared.mark_redraw();
        name
    };
    // 锁外写盘（阻塞线程池）+ 续发重载
    match write_config_blocking(shared).await {
        Ok(()) => {
            shared.log(LogType::Info, format!("插入订阅：{name}"));
            reload_config_impl(shared).await;
        }
        Err(e) => shared.log(LogType::Error, e.to_string()),
    }
}
