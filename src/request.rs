use serde::{Deserialize, Serialize};
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct TrackerResponse {
    pub interval: usize,
    #[serde(with = "serde_bytes")]
    pub peers: Vec<u8>,
}
