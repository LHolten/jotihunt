use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct AtomicEdit {
    pub key: String,
    pub old: String,
    pub new: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Broadcast {
    pub key: String,
    pub new: String,
}
