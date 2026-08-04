use serde::{Deserialize, Serialize};

#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub speed: String,
}
impl Node {
    pub fn new(name: String) -> Self {
        Self {
            name: name,
            speed: "-".to_string(),
        }
    }
}

//用于接收mihomo的回复用的节点
#[derive(Debug, Serialize, Deserialize)]
pub struct ProxyReport {
    pub alive: bool,
    pub all: Vec<String>,
    #[serde(rename = "dialer-proxy")]
    pub dialer_proxy: String,
    pub hidden: bool,
    pub icon: String,
    pub interface: String,
    pub name: String,
    pub now: String,
    #[serde(rename = "type")]
    pub node_type: String,
}
