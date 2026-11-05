//! Serializes hardware operations on one worker thread. Construction never
//! opens the receiver or changes its settings. Driver mode changes are
//! journaled before the write and restored on quit.

use super::onboard::{self, Functions, OnboardProfilePlan};
use super::protocol::{HardwareSnapshot, RazerCommand, RazerError, RazerTransport, Session};
use crate::model::data_folder;
use crate::model::store;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::mpsc;

#[derive(Clone, Debug)]
pub enum Operation {
    Refresh,
    Dpi(i32, i32),
    Polling(i32),
    Mode(bool),
    /// Shutdown: restore the mode changed in this session.
    Restore,
    /// Explicit recovery of a mode left by an earlier session.
    Recover,
    Onboard(Box<OnboardProfilePlan>),
    RestoreOnboard,
    ImportOnboard,
}

#[derive(Clone, Debug, Default)]
pub struct Outcome {
    pub connected: bool,
    pub snapshot: Option<HardwareSnapshot>,
    pub message: String,
    pub recovery_pending: bool,
    pub imported: Option<Functions>,
    pub imported_hypershift: Option<Functions>,
}

#[derive(Serialize, Deserialize)]
struct Recovery {
    identity: String,
    #[serde(rename = "originalMode")]
    original_mode: u8,
}

pub type TransportFactory = Box<dyn FnMut() -> Result<Box<dyn RazerTransport>, RazerError> + Send>;

pub struct Worker {
    journal: PathBuf,
    onboard_journal: PathBuf,
    mode_changed_this_session: bool,
    open: TransportFactory,
}

impl Worker {
    pub fn new(open: TransportFactory) -> Self {
        Worker {
            journal: data_folder::file("driver-mode-recovery.json"),
            onboard_journal: onboard::journal_path(),
            mode_changed_this_session: false,
            open,
        }
    }

    #[cfg(test)]
    pub fn with_paths(open: TransportFactory, journal: PathBuf, onboard_journal: PathBuf) -> Self {
        Worker { journal, onboard_journal, mode_changed_this_session: false, open }
    }

    fn pending(&self) -> bool {
        self.journal.exists()
    }

    pub fn perform(&mut self, operation: &Operation) -> Outcome {
        // Shutdown without an outstanding mode change needs no USB I/O.
        if matches!(operation, Operation::Restore) && !self.mode_changed_this_session {
            return Outcome { message: "No mode to restore.".into(), recovery_pending: self.pending(), ..Default::default() };
        }
        let mut connected = false;
        let result = self.run(operation, &mut connected);
        result.unwrap_or_else(|error| Outcome {
            connected,
            message: error.to_string(),
            recovery_pending: self.pending(),
            ..Default::default()
        })
    }

    fn run(&mut self, operation: &Operation, connected: &mut bool) -> Result<Outcome, RazerError> {
        // Validate user input before opening any device.
        match operation {
            Operation::Dpi(x, y) => {
                RazerCommand::set_dpi(*x, *y)?;
            }
            Operation::Polling(hz) => {
                RazerCommand::set_polling(*hz)?;
            }
            _ => {}
        }
        let mut transport = (self.open)()?;
        *connected = true;
        let identity = transport.identity();
        let mut session = Session::new(transport.as_mut());
        let mut outcome = Outcome::default();
        let mut onboard_message = None;
        match operation {
            Operation::Refresh => {}
            Operation::Dpi(x, y) => session.set_dpi(*x, *y)?,
            Operation::Polling(hz) => session.set_polling(*hz)?,
            Operation::Onboard(plan) => {
                if self.pending() {
                    return Err(RazerError::InvalidValue("Restore the original driver mode first.".into()));
                }
                onboard::save(plan, &mut session, &identity, &self.onboard_journal)?;
                onboard_message = Some(format!("Profile {} saved to the mouse. It keeps working after Quit.", plan.name));
            }
            Operation::RestoreOnboard => {
                onboard::restore(&mut session, &identity, &self.onboard_journal)?;
                onboard_message = Some("Previous assignments restored. Software remapping is available.".into());
            }
            Operation::ImportOnboard => {
                let standard = onboard::read_functions(&mut session, 0, 0)?;
                // The Hypershift layer is reachable only through a Hypershift key.
                if standard.values().any(|f| f.as_slice() == onboard::HYPERSHIFT_FUNCTION) {
                    outcome.imported_hypershift = Some(onboard::read_functions(&mut session, 0, 1)?);
                }
                outcome.imported = Some(standard);
            }
            Operation::Mode(enabled) => {
                let current = session.read_mode()?;
                let target = if *enabled { 3 } else { 0 };
                if current != target {
                    if self.pending() {
                        let saved = self.read_recovery()?;
                        if saved.identity != identity {
                            return Err(RazerError::Transport("A restore is pending for another receiver. No mode was changed.".into()));
                        }
                        if !self.mode_changed_this_session {
                            return Err(RazerError::Transport(
                                "Explicitly restore the previous session's mode before changing it.".into(),
                            ));
                        }
                    } else {
                        // The journal write must succeed before the mode SET.
                        store::write_json(&Recovery { identity: identity.clone(), original_mode: current }, &self.journal)
                            .map_err(|e| RazerError::Transport(format!("Could not write the recovery journal: {e}")))?;
                    }
                    // A timeout can follow a successful device write, so
                    // restoration is required even if readback fails.
                    self.mode_changed_this_session = true;
                    session.set_mode(target)?;
                }
            }
            Operation::Restore | Operation::Recover => {
                let saved = self.read_recovery()?;
                if saved.identity != identity {
                    return Err(RazerError::Transport("Reconnect the receiver to its original USB port to restore the mode.".into()));
                }
                if session.read_mode()? != saved.original_mode {
                    session.set_mode(saved.original_mode)?;
                }
                std::fs::remove_file(&self.journal).map_err(|e| RazerError::Transport(e.to_string()))?;
                self.mode_changed_this_session = false;
            }
        }
        let snapshot = session.read_snapshot()?;
        let recovery_pending = self.pending();
        let message = if recovery_pending {
            if self.mode_changed_this_session {
                "Values read. The original mode will be restored when you quit.".to_string()
            } else {
                "A restore is pending from a previous session. Use Restore Original Mode.".to_string()
            }
        } else {
            "Hardware values read over USB.".to_string()
        };
        let mut lines = vec![onboard_message.unwrap_or(message)];
        lines.extend(snapshot.warnings.iter().cloned());
        outcome.connected = true;
        outcome.message = lines.join("\n");
        outcome.recovery_pending = recovery_pending;
        outcome.snapshot = Some(snapshot);
        Ok(outcome)
    }

    fn read_recovery(&self) -> Result<Recovery, RazerError> {
        let data = std::fs::read(&self.journal).map_err(|e| RazerError::InvalidValue(format!("Invalid restore record: {e}.")))?;
        let saved: Recovery = serde_json::from_slice(&data).map_err(|_| RazerError::InvalidValue("Invalid restore record.".into()))?;
        if saved.original_mode != 0 && saved.original_mode != 3 {
            return Err(RazerError::InvalidValue("Invalid restore record.".into()));
        }
        Ok(saved)
    }
}

pub struct Request {
    pub operation: Operation,
    pub ticket: u64,
}

pub struct Response {
    pub operation: Operation,
    pub ticket: u64,
    pub outcome: Outcome,
}

/// Starts the worker thread. Responses go to `deliver`, which must not
/// block (the UI forwards them to its main loop).
pub fn spawn(mut worker: Worker, deliver: impl Fn(Response) + Send + 'static) -> mpsc::Sender<Request> {
    let (tx, rx) = mpsc::channel::<Request>();
    std::thread::Builder::new()
        .name("Naga hardware".into())
        .spawn(move || {
            while let Ok(request) = rx.recv() {
                let outcome = worker.perform(&request.operation);
                deliver(Response { operation: request.operation, ticket: request.ticket, outcome });
            }
        })
        .expect("hardware worker thread");
    tx
}

/// Main-thread state of the hardware facade.
#[derive(Clone, Debug)]
pub struct DeviceState {
    pub is_connected: bool,
    pub is_busy: bool,
    pub dpi_x: Option<i32>,
    pub dpi_y: Option<i32>,
    pub polling_rate: Option<i32>,
    pub battery_level: Option<i32>,
    pub driver_mode_enabled: bool,
    pub recovery_pending: bool,
    pub status_message: String,
    pub shutting_down: bool,
}

impl Default for DeviceState {
    fn default() -> Self {
        DeviceState {
            is_connected: false,
            is_busy: false,
            dpi_x: None,
            dpi_y: None,
            polling_rate: None,
            battery_level: None,
            driver_mode_enabled: false,
            recovery_pending: data_folder::file("driver-mode-recovery.json").exists(),
            status_message: "Connect the USB receiver and press Refresh.".into(),
            shutting_down: false,
        }
    }
}

impl DeviceState {
    /// Whether a request may start. Stale slider writes are never queued;
    /// shutdown restoration is the one request allowed behind another.
    pub fn admit(&mut self, operation: &Operation) -> bool {
        let shutdown = matches!(operation, Operation::Restore);
        if shutdown {
            self.shutting_down = true;
        } else if self.shutting_down || self.is_busy {
            return false;
        }
        self.is_busy = true;
        self.status_message = "Talking to the mouse…".into();
        true
    }

    pub fn complete(&mut self, outcome: &Outcome) {
        self.is_busy = false;
        self.is_connected = outcome.connected;
        let snapshot = outcome.snapshot.clone().unwrap_or_default();
        self.dpi_x = snapshot.dpi_x;
        self.dpi_y = snapshot.dpi_y;
        self.polling_rate = snapshot.polling_rate;
        self.battery_level = snapshot.battery_level;
        self.driver_mode_enabled = snapshot.mode == Some(3);
        self.recovery_pending = outcome.recovery_pending;
        self.status_message = outcome.message.clone();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hw::onboard::tests::FakeMouse;
    use std::sync::{Arc, Mutex};

    struct Shared(Arc<Mutex<FakeMouse>>);
    impl RazerTransport for Shared {
        fn identity(&self) -> String {
            self.0.lock().unwrap().identity()
        }
        fn exchange(&mut self, request: &[u8]) -> Result<Vec<u8>, RazerError> {
            let mut mouse = self.0.lock().unwrap();
            // The fake answers DPI, polling and battery too.
            match (request[6], request[7]) {
                (4, 0x85) | (0, 0x85) | (7, 0x80) | (4, 5) | (0, 5) | (0, 4) => {
                    let mut bytes = vec![0u8; 90];
                    bytes[0] = 2;
                    bytes[1] = request[1];
                    bytes[5] = request[5];
                    bytes[6] = request[6];
                    bytes[7] = request[7];
                    let args: Vec<u8> = match (request[6], request[7]) {
                        (4, 0x85) => vec![0, 6, 0x40, 6, 0x40, 0, 0],
                        (0, 0x85) => vec![1],
                        (7, 0x80) => vec![0, 200],
                        (0, 4) => {
                            mouse.mode = request[8];
                            vec![request[8], 0]
                        }
                        _ => request[8..8 + request[5] as usize].to_vec(),
                    };
                    bytes[8..8 + args.len()].copy_from_slice(&args);
                    bytes[88] = crate::hw::protocol::checksum(&bytes);
                    Ok(bytes)
                }
                _ => mouse.exchange(request),
            }
        }
    }

    fn worker(name: &str) -> (Worker, Arc<Mutex<FakeMouse>>, PathBuf) {
        let dir = crate::model::store::tests::temp_dir(name);
        let mouse = Arc::new(Mutex::new(FakeMouse::factory()));
        let shared = mouse.clone();
        let journal = dir.join("driver-mode-recovery.json");
        let worker = Worker::with_paths(
            Box::new(move || Ok(Box::new(Shared(shared.clone())) as Box<dyn RazerTransport>)),
            journal.clone(),
            dir.join("onboard-profile.json"),
        );
        (worker, mouse, journal)
    }

    #[test]
    fn refresh_reads_values() {
        let (mut worker, _, _) = worker("hw-refresh");
        let outcome = worker.perform(&Operation::Refresh);
        assert!(outcome.connected);
        let snapshot = outcome.snapshot.unwrap();
        assert_eq!((snapshot.dpi_x, snapshot.polling_rate, snapshot.battery_level, snapshot.mode), (Some(1600), Some(1000), Some(78), Some(0)));
        assert_eq!(outcome.message, "Hardware values read over USB.");
    }

    #[test]
    fn invalid_input_never_opens_the_device() {
        let dir = crate::model::store::tests::temp_dir("hw-invalid");
        let mut worker = Worker::with_paths(Box::new(|| panic!("opened")), dir.join("j"), dir.join("o"));
        assert!(worker.perform(&Operation::Dpi(50, 50)).message.contains("100"));
        assert!(worker.perform(&Operation::Polling(250)).message.contains("125"));
        assert_eq!(worker.perform(&Operation::Restore).message, "No mode to restore.");
    }

    #[test]
    fn driver_mode_is_journaled_and_restored() {
        let (mut worker, mouse, journal) = worker("hw-mode");
        let outcome = worker.perform(&Operation::Mode(true));
        assert!(journal.exists(), "journal written before the change");
        assert_eq!(mouse.lock().unwrap().mode, 3);
        assert!(outcome.recovery_pending);
        assert!(outcome.message.contains("restored when you quit"));
        let outcome = worker.perform(&Operation::Restore);
        assert_eq!(mouse.lock().unwrap().mode, 0);
        assert!(!journal.exists());
        assert!(!outcome.recovery_pending);
    }

    #[test]
    fn earlier_session_needs_explicit_recovery() {
        let (mut worker, mouse, journal) = worker("hw-recover");
        worker.perform(&Operation::Mode(true));
        let (mut next, _, _) = worker_with_journal(mouse.clone(), journal.clone());
        assert!(next.perform(&Operation::Mode(false)).message.contains("Explicitly restore"));
        assert_eq!(next.perform(&Operation::Restore).message, "No mode to restore.");
        next.perform(&Operation::Recover);
        assert_eq!(mouse.lock().unwrap().mode, 0);
        assert!(!journal.exists());
    }

    fn worker_with_journal(mouse: Arc<Mutex<FakeMouse>>, journal: PathBuf) -> (Worker, (), ()) {
        let shared = mouse.clone();
        let onboard = journal.with_file_name("onboard-profile.json");
        (Worker::with_paths(Box::new(move || Ok(Box::new(Shared(shared.clone())) as Box<dyn RazerTransport>)), journal, onboard), (), ())
    }

    #[test]
    fn import_reads_both_layers_when_needed() {
        let (mut worker, _, _) = worker("hw-import");
        let outcome = worker.perform(&Operation::ImportOnboard);
        assert_eq!(outcome.imported.unwrap().len(), 21);
        assert!(outcome.imported_hypershift.is_none());
    }

    #[test]
    fn facade_admission() {
        let mut state = DeviceState::default();
        assert!(state.admit(&Operation::Refresh));
        assert!(!state.admit(&Operation::Dpi(800, 800)), "no queued writes");
        assert!(state.admit(&Operation::Restore), "shutdown restore always runs");
        state.complete(&Outcome { connected: true, ..Default::default() });
        assert!(!state.admit(&Operation::Refresh), "nothing after shutdown starts");
    }
}
