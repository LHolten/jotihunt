use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct State {
    pub data: BTreeMap<Address, Fox>,
    pub current_time: String,
}

#[derive(Debug, Default)]
pub struct TimeSlice {
    pub foxes: BTreeMap<String, Fox>,
}

#[derive(Debug, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Address {
    pub time_slice: String,
    pub fox_name: String,
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Fox {
    pub latitude: String,
    pub longitude: String,
}
