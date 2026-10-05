pub mod domain;

use domain::Fox;
use serde::{Deserialize, Serialize};

/// Body of `PUT /{key}/locations/{day}/{time}/{fox_name}`,
/// fails if the current location is not `old`.
///
/// `POST` (create) has the new [Fox] as body and `DELETE` the old [Fox].
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ModifyLocation {
    pub old: Fox,
    pub new: Fox,
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
