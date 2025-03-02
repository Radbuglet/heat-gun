use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MpSbHello {
    pub username: String,
    pub style: u8,
}
