//! The receiver's control channel: protocol, hidraw transport, mouse
//! memory and the worker that serializes hardware operations.

pub mod controller;
pub mod descriptor;
pub mod hidraw;
pub mod onboard;
pub mod protocol;
pub mod sysfs;
