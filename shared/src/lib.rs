use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct AtomicEdit {
    pub key: Vec<u8>,
    pub old: Vec<u8>,
    pub new: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Broadcast {
    pub key: Vec<u8>,
    pub value: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Traccar {
    pub id: String,
    pub lat: String,
    pub lon: String,
}
