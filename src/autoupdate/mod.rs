pub mod config;
pub mod github;
pub mod manual;
pub mod runner;
pub mod settings_io;
pub mod status;
pub mod systemd;

#[allow(unused_imports)]
pub use config::AutoupdateConfig;
pub use manual::{ManualCheckResult, trigger_manual};
pub use runner::spawn;
pub use status::{UpdateStatus, read_status};
