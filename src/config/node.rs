use serde::{Deserialize, Serialize};

/// 节点列表展示模型（AppState.nodes）
#[derive(Debug, Serialize, Deserialize)]
pub struct Node {
    pub name: String,
    pub speed: String,
}

impl Node {
    pub fn new(name: String) -> Self {
        Self {
            name,
            speed: "-".to_string(),
        }
    }
}
