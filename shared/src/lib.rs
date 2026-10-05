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

/// JSON data of the server sent events of a table.
#[derive(Serialize, Deserialize, Debug)]
pub enum Broadcast<K, V> {
    /// Sent at the start of every connection, before all the rows.
    /// Clients reconnect automatically, so they should forget all rows they have.
    Reset,
    /// `None` means that the row was removed.
    Update { key: K, value: Option<V> },
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct Traccar {
    pub id: String,
    pub lat: String,
    pub lon: String,
}
