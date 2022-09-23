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

// let time_slice = TimeSlice {
//     foxes: vec![(
//         "Alpha".to_owned(),
//         Fox {
//             latitude: "-".to_owned(),
//             longitude: "-".to_owned(),
//         },
//     )]
//     .into_iter()
//     .collect(),
// };
// let data = Data {
//     time_slices: vec![
//         ("9:00".to_owned(), time_slice),
//         ("10:00".to_owned(), Default::default()),
//     ]
//     .into_iter()
//     .collect(),
//     current_time: "9:00".to_owned(),
// };
