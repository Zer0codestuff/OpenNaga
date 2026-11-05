//! Platform-independent logic: actions, profiles, validation, storage,
//! remapping semantics and macros. Nothing here touches devices or GTK.

pub mod actions;
pub mod app_profiles;
pub mod config;
pub mod data_folder;
pub mod import;
pub mod layout;
pub mod keys;
pub mod macro_runner;
pub mod mapper;
pub mod prefs;
pub mod profile;
pub mod scroll;
pub mod store;
pub mod validation;
