// #[repr(u8)]
// pub enum Sector {
//     Alpha,
//     Bravo,
//     Charlie,
//     Delta,
//     Echo,
//     Foxtrot,
// }
// const SECTORS: [&'static str; 6] = ["Alpha", "Bravo", "Charlie", "Delta", "Echo", "Foxtrot"];

// pub struct Time {
//     hour: u8,
//     half: bool,
// }

pub struct Location {
    lat: String,
    lng: String,
}

pub enum Axis {
    Latitude,
    Longitude,
}

pub struct AtomicEdit {
    // sector: usize,
    // time: usize,
    // axis: Axis,
    value: String,
    old: String,
    new: String,
}
