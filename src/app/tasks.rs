use crate::app::Manager;
use crate::app::ui::WindowsManager;
use crate::app::ui::pages::main::MAIN;
use crate::command::mihomo;
use crate::config::node::Node;
use crate::operation_log::LogType;
use crate::settings::Settings;
use std::path::PathBuf;
use tokio::sync::mpsc;

/// 异步任务回传的状态更新闭包，在主线程 `poll()` 中执行
pub type AsyncTask = Box<dyn FnOnce(&mut Manager, &mut WindowsManager) + Send>;

/// 启动 mihomo 后轮询端口就绪，就绪后自动刷新节点；
/// 超时则提示进程可能启动失败（PID 已记录，可按 s 停止）
pub fn start_and_wait(tx: mpsc::Sender<AsyncTask>, settings: Settings) {
    tokio::spawn(async move {
        let attempts = settings.provider_retry.max(1);
        let interval = settings.provider_retry_interval();
        for _ in 0..attempts {
            if mihomo::is_port_up(&settings) {
                break;
            }
            tokio::time::sleep(interval).await;
        }
        let ready = mihomo::is_port_up(&settings);
        let _ = tx
            .send(Box::new(move |m, _windows| {
                if ready {
                    m.logs.add(LogType::Info, "mihomo 已就绪，正在拉取节点".to_string());
                    let tx = m.tasks.tx.clone();
                    reflash_nodes(tx, settings);
                } else {
                    m.logs.add(
                        LogType::Warn,
                        "进程已启动但端口未就绪（启动可能较慢或失败），可按 s 停止后重试"
                            .to_string(),
                    );
                }
            }))
            .await;
    });
}

pub fn delay(tx: mpsc::Sender<AsyncTask>, settings: Settings) {
    tokio::spawn(async move {
        let result = mihomo::fetch_delays(&settings).await;
        let _ = tx
            .send(Box::new(move |m, windows| match result {
                Ok(map) => {
                    for node in &mut windows.main.content.nodes {
                        if let Some(&d) = map.get(&node.name) {
                            node.speed = format!("{d}ms");
                        } else {
                            node.speed = "-".to_string();
                        }
                    }
                    m.logs.add(LogType::Info, "测速完成".to_string());
                    m.mihomo.is_test_delay = false;
                }
                Err(e) => {
                    m.mihomo.is_test_delay = false;
                    m.logs.add(LogType::Error, e);
                }
            }))
            .await;
    });
}

pub fn reflash_nodes(tx: mpsc::Sender<AsyncTask>, settings: Settings) {
    tokio::spawn(async move {
        let result = mihomo::get_proxy(&settings).await;
        let _ = tx
            .send(Box::new(move |m, windows| match result {
                Ok(proxy) => {
                    windows.main.content.nodes = vec![];
                    windows.main.content.select = 0;
                    m.mihomo.active_node = None;
                    for (index, node) in proxy.all.into_iter().enumerate() {
                        if node == proxy.now {
                            m.mihomo.active_node = Some(index);
                            windows.main.content.select = index;
                        }
                        windows.main.content.nodes.push(Node::new(node));
                    }
                    m.logs.add(LogType::Info, "更新代理信息".to_string());
                }
                Err(e) => {
                    m.logs.add(LogType::Error, e);
                }
            }))
            .await;
    });
}

pub fn switch_node(tx: mpsc::Sender<AsyncTask>, settings: Settings, name: String) {
    tokio::spawn(async move {
        let result = mihomo::switch_node(&settings, name.clone()).await;
        let _ = tx
            .send(Box::new(move |m, _windows| match result {
                Ok(_) => {
                    m.logs.add(LogType::Info, format!("切换节点：{}", name));
                }
                Err(e) => {
                    m.logs.add(LogType::Error, e);
                }
            }))
            .await;
    });
}

pub fn reload(tx: mpsc::Sender<AsyncTask>, settings: Settings, path: PathBuf) {
    tokio::spawn(async move {
        let result = mihomo::reload_config(&settings, path).await;
        let _ = tx
            .send(Box::new(move |m, _windows| match result {
                Ok(_) => {
                    m.current_window = MAIN;
                    m.logs.add(LogType::Info, "重置配置成功".to_string());
                    let tx = m.tasks.tx.clone();
                    reflash_nodes(tx, settings);
                }
                Err(e) => {
                    m.logs.add(LogType::Error, e);
                }
            }))
            .await;
    });
}

pub fn insert_sub(tx: mpsc::Sender<AsyncTask>, settings: Settings, url: String) {
    tokio::spawn(async move {
        let result = mihomo::get_provider_name(&settings, url.clone()).await;
        let _ = tx
            .send(Box::new(move |m, _windows| {
                let name = match result {
                    Ok(name) => name,
                    Err(e) => {
                        let n = m
                            .config
                            .config
                            .proxy_providers
                            .as_ref()
                            .map(|p| p.len())
                            .unwrap_or(0)
                            + 1;
                        let fallback = format!("订阅{n}");
                        m.logs.add(
                            LogType::Warn,
                            format!("{}，使用默认名称 {}", e, fallback),
                        );
                        fallback
                    }
                };
                m.current_window = MAIN;

                match m
                    .config
                    .config
                    .insert_sub(url, name.clone(), &m.config.config_path)
                {
                    Ok(_) => {
                        m.logs.add(LogType::Info, format!("插入代理商：{}", name));
                        let tx = m.tasks.tx.clone();
                        reload(tx, settings, m.config.config_path.clone());
                    }
                    Err(e) => m.logs.add(LogType::Error, e),
                }
            }))
            .await;
    });
}
