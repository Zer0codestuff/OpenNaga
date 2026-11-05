//! The input thread. One epoll loop owns the Naga's evdev nodes, its hidraw
//! report streams, hotplug notifications, macro timers and the virtual
//! output devices. Nothing here waits on the UI.
//!
//! Remapping grabs a node (EVIOCGRAB) so the desktop no longer sees it, then
//! performs mapped controls and forwards everything else unchanged through
//! the virtual devices. Pointer nodes are grabbed only when a pointer
//! control is remapped or scroll sensitivity is changed; otherwise pointer
//! motion never passes through OpenNaga.

use super::evdev::{self, EvdevDevice, InputEvent};
use super::frame::{NodeState, Notice, RawKind, Remapper};
use super::output::{Output, OutputEvent, KEYBOARD_NAME, POINTER_NAME};
use super::typing::TypingMap;
use crate::hw::sysfs::{self, HidDevice};
use crate::model::actions::SystemAction;
use crate::model::config::MappingUpdate;
use crate::model::keys::ev;
use crate::model::mapper::MapperOutput;
use crate::model::profile::Mapping;
use crate::model::scroll::ScrollSensitivity;
use std::collections::{BTreeMap, BTreeSet};
use std::fs::{File, OpenOptions};
use std::io::Read;
use std::os::fd::{AsRawFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use std::time::{Duration, Instant};

pub use super::frame::Policy;

pub enum Command {
    Mapping(MappingUpdate),
    Policy(Policy),
    Scroll(ScrollSensitivity),
    DriverMode(bool),
    Typing(TypingMap),
    CancelMacros,
    TestSystem(SystemAction),
    /// The settings window is visible: also watch pointer nodes for activity.
    Observe(bool),
    Suspend(bool),
    Rescan,
    Shutdown,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EngineStatus {
    pub running: bool,
    pub virtual_error: Option<String>,
    pub naga_nodes: usize,
    pub readable_nodes: usize,
    pub grabbed_nodes: usize,
    pub hidraw_nodes: usize,
    pub hidraw_readable: usize,
    pub keyboards_total: usize,
    pub keyboards_readable: usize,
    pub remapping: bool,
    pub suspended: bool,
}

impl EngineStatus {
    pub fn input_access(&self) -> bool {
        self.naga_nodes > 0 && self.readable_nodes == self.naga_nodes
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceReport {
    pub name: Option<String>,
    pub transport: Option<String>,
    pub bluetooth_battery: Option<i32>,
}

pub enum Event {
    Devices(DeviceReport),
    Status(EngineStatus),
    Activity(i32),
    LastInput(String),
    Macros { active: usize, error: Option<String> },
    SwitchProfile(String),
    SystemResult(Option<String>),
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Role {
    Keyboard,
    Pointer,
}

struct Node {
    device: EvdevDevice,
    role: Role,
    grabbed: bool,
    polled: bool,
    frame: Vec<InputEvent>,
    state: NodeState,
}

struct RawNode {
    file: File,
    kind: RawKind,
    source: String,
    polled: bool,
}

/// Sends commands to the input thread from any thread.
#[derive(Clone)]
pub struct EngineSender {
    tx: mpsc::Sender<Command>,
    wake: RawFd,
}

impl EngineSender {
    pub fn send(&self, command: Command) {
        if self.tx.send(command).is_ok() {
            let one: u64 = 1;
            unsafe { libc::write(self.wake, &one as *const u64 as *const libc::c_void, 8) };
        }
    }
}

pub struct EngineHandle {
    tx: mpsc::Sender<Command>,
    wake: RawFd,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl EngineHandle {
    pub fn sender(&self) -> EngineSender {
        EngineSender { tx: self.tx.clone(), wake: self.wake }
    }

    pub fn send(&self, command: Command) {
        self.sender().send(command);
    }

    pub fn shutdown(&mut self) {
        self.send(Command::Shutdown);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

const WAKE_TOKEN: u64 = u64::MAX;
const INOTIFY_TOKEN: u64 = u64::MAX - 1;

pub fn spawn(emit: impl Fn(Event) + Send + Sync + 'static) -> EngineHandle {
    let (tx, rx) = mpsc::channel();
    let wake = unsafe { libc::eventfd(0, libc::EFD_NONBLOCK | libc::EFD_CLOEXEC) };
    let emit: Arc<dyn Fn(Event) + Send + Sync> = Arc::new(emit);
    let thread = std::thread::Builder::new()
        .name("Naga input".into())
        .spawn(move || {
            raise_priority();
            let mut engine = Engine::new(rx, wake, emit);
            engine.run();
        })
        .expect("input thread");
    EngineHandle { tx, wake, thread: Some(thread) }
}

/// Input handling should stay responsive under load. Real-time scheduling
/// is used when the user's limits allow it; otherwise this is a no-op.
fn raise_priority() {
    unsafe {
        let param = libc::sched_param { sched_priority: 10 };
        if libc::pthread_setschedparam(libc::pthread_self(), libc::SCHED_RR, &param) != 0 {
            libc::setpriority(libc::PRIO_PROCESS, 0, -5);
        }
    }
}

struct Engine {
    rx: mpsc::Receiver<Command>,
    wake: RawFd,
    epoll: RawFd,
    inotify: RawFd,
    emit: Arc<dyn Fn(Event) + Send + Sync>,
    nodes: BTreeMap<RawFd, Node>,
    raws: BTreeMap<RawFd, RawNode>,
    out: Output,
    rm: Remapper,
    notices: Vec<Notice>,
    standard: Mapping,
    hypershift: Mapping,
    observe: bool,
    suspended: bool,
    rescan_at: Option<Instant>,
    battery_at: Instant,
    status: EngineStatus,
    devices: DeviceReport,
    access: AccessCounts,
    stop: bool,
}

#[derive(Default)]
struct AccessCounts {
    naga_nodes: usize,
    readable_nodes: usize,
    hidraw_nodes: usize,
    hidraw_readable: usize,
    keyboards_total: usize,
}

impl Engine {
    fn new(rx: mpsc::Receiver<Command>, wake: RawFd, emit: Arc<dyn Fn(Event) + Send + Sync>) -> Self {
        let epoll = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        let inotify = unsafe { libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC) };
        let mask = libc::IN_CREATE | libc::IN_DELETE | libc::IN_ATTRIB;
        unsafe {
            libc::inotify_add_watch(inotify, c"/dev/input".as_ptr(), mask);
            libc::inotify_add_watch(inotify, c"/dev".as_ptr(), mask);
        }
        let forward = emit.clone();
        let out = Output::new(Arc::new(move |event| {
            forward(match event {
                OutputEvent::SwitchProfile(name) => Event::SwitchProfile(name),
                OutputEvent::Error(message) => Event::Error(message),
            })
        }));
        let mut engine = Engine {
            rx,
            wake,
            epoll,
            inotify,
            emit,
            nodes: BTreeMap::new(),
            raws: BTreeMap::new(),
            out,
            rm: Remapper { scroll: ScrollSensitivity::BASELINE, ..Default::default() },
            notices: Vec::new(),
            standard: Mapping::new(),
            hypershift: Mapping::new(),
            observe: false,
            suspended: false,
            rescan_at: None,
            battery_at: Instant::now(),
            status: EngineStatus::default(),
            devices: DeviceReport::default(),
            access: AccessCounts::default(),
            stop: false,
        };
        engine.watch(wake, WAKE_TOKEN);
        engine.watch(inotify, INOTIFY_TOKEN);
        engine.rescan();
        engine
    }

    fn watch(&self, fd: RawFd, token: u64) {
        let mut event = libc::epoll_event { events: libc::EPOLLIN as u32, u64: token };
        unsafe { libc::epoll_ctl(self.epoll, libc::EPOLL_CTL_ADD, fd, &mut event) };
    }

    fn unwatch(&self, fd: RawFd) {
        unsafe { libc::epoll_ctl(self.epoll, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) };
    }

    fn run(&mut self) {
        let mut events: [libc::epoll_event; 32] = unsafe { std::mem::zeroed() };
        while !self.stop {
            let timeout = self.timeout();
            let count = unsafe { libc::epoll_wait(self.epoll, events.as_mut_ptr(), events.len() as i32, timeout) };
            if count < 0 {
                if std::io::Error::last_os_error().raw_os_error() == Some(libc::EINTR) {
                    continue;
                }
                break;
            }
            for event in events.iter().take(count as usize) {
                match event.u64 {
                    WAKE_TOKEN => self.drain_commands(),
                    INOTIFY_TOKEN => self.drain_inotify(),
                    token => {
                        let fd = token as RawFd;
                        if self.nodes.contains_key(&fd) {
                            self.read_node(fd);
                        } else if self.raws.contains_key(&fd) {
                            self.read_raw(fd);
                        }
                    }
                }
            }
            let now = Instant::now();
            if self.rescan_at.map(|t| t <= now).unwrap_or(false) {
                self.rescan_at = None;
                self.rescan();
            }
            if now >= self.battery_at {
                self.battery_at = now + Duration::from_secs(60);
                self.report_devices(false);
            }
            for token in self.out.reap() {
                self.rm.mapper.resume_macro(token, now);
            }
            self.rm.mapper.tick_macros(&mut self.out);
            self.out.flush();
            self.publish_macros();
        }
        self.shutdown();
    }

    fn timeout(&self) -> i32 {
        let now = Instant::now();
        let mut deadline = self.battery_at;
        if let Some(t) = self.rm.mapper.next_macro_deadline() {
            deadline = deadline.min(t);
        }
        if let Some(t) = self.rescan_at {
            deadline = deadline.min(t);
        }
        let mut ms = deadline.saturating_duration_since(now).as_millis().min(60_000) as i32;
        if self.out.waiting_on_helpers() {
            ms = ms.min(20);
        }
        ms
    }

    fn drain_commands(&mut self) {
        let mut buffer = [0u8; 8];
        unsafe { libc::read(self.wake, buffer.as_mut_ptr() as *mut libc::c_void, 8) };
        while let Ok(command) = self.rx.try_recv() {
            match command {
                Command::Mapping(update) => {
                    self.standard = update.standard.clone();
                    self.hypershift = update.hypershift.clone();
                    if update.preserve_holds {
                        self.rm.mapper.swap_mapping(update.standard, update.hypershift);
                    } else {
                        self.rm.active_down.clear();
                        self.rm.mapper.update_mapping(update.standard, update.hypershift, &mut self.out);
                    }
                    self.apply_policy();
                }
                Command::Policy(policy) => {
                    if policy != self.rm.policy {
                        if !policy.remap {
                            self.release_mapped();
                        }
                        self.rm.policy = policy;
                        self.apply_policy();
                    }
                }
                Command::Scroll(sensitivity) => {
                    self.rm.scroll = sensitivity.clamped();
                    self.rm.mapper.scroll_sensitivity = self.rm.scroll;
                    self.rm.wheel.reset();
                    self.apply_policy();
                }
                Command::DriverMode(enabled) => {
                    if enabled != self.rm.driver_mode {
                        self.rm.driver_mode = enabled;
                        self.rm.driver_state.reset();
                        self.release_mapped();
                        self.apply_policy();
                    }
                }
                Command::Typing(map) => self.out.typing = map,
                Command::CancelMacros => self.rm.mapper.cancel_macros(),
                Command::TestSystem(action) => {
                    let result = self.rm.mapper.perform_system(action, &mut self.out);
                    self.out.flush();
                    (self.emit)(Event::SystemResult(result));
                }
                Command::Observe(on) => {
                    self.observe = on;
                    self.apply_policy();
                }
                Command::Suspend(on) => {
                    self.suspended = on;
                    if on {
                        self.release_mapped();
                        self.rm.driver_state.reset();
                    } else {
                        self.rescan();
                    }
                    self.apply_policy();
                }
                Command::Rescan => {
                    self.release_mapped();
                    self.close_all();
                    self.out.create_devices();
                    self.rescan();
                }
                Command::Shutdown => self.stop = true,
            }
        }
    }

    fn drain_inotify(&mut self) {
        let mut buffer = [0u8; 4096];
        let mut relevant = false;
        loop {
            let count = unsafe { libc::read(self.inotify, buffer.as_mut_ptr() as *mut libc::c_void, buffer.len()) };
            if count <= 0 {
                break;
            }
            let mut offset = 0usize;
            while offset + 16 <= count as usize {
                let length = u32::from_ne_bytes(buffer[offset + 12..offset + 16].try_into().unwrap()) as usize;
                let name = &buffer[offset + 16..(offset + 16 + length).min(count as usize)];
                let name = String::from_utf8_lossy(name);
                let name = name.trim_end_matches('\0');
                if name.starts_with("event") || name.starts_with("hidraw") || name == "uinput" {
                    relevant = true;
                }
                offset += 16 + length;
            }
        }
        if relevant {
            // udev applies permissions after the node appears.
            self.rescan_at = Some(Instant::now() + Duration::from_millis(350));
        }
    }

    // MARK: - Devices

    fn rescan(&mut self) {
        let hid = sysfs::naga_devices();
        let wanted_events: BTreeSet<PathBuf> = hid.iter().flat_map(|d| d.events.iter().cloned()).collect();
        let wanted_raw: BTreeSet<PathBuf> = hid.iter().filter_map(|d| d.hidraw.clone()).collect();
        // Close nodes that disappeared.
        let gone: Vec<RawFd> = self.nodes.iter().filter(|(_, n)| !wanted_events.contains(&n.device.path)).map(|(fd, _)| *fd).collect();
        for fd in gone {
            self.close_node(fd);
        }
        let gone_raw: Vec<RawFd> = self.raws.iter().filter(|(_, r)| !wanted_raw.contains(&PathBuf::from(&r.source))).map(|(fd, _)| *fd).collect();
        for fd in gone_raw {
            self.unwatch(fd);
            self.raws.remove(&fd);
            self.rm.driver_state.reset();
        }
        let mut access = AccessCounts::default();
        for device in &hid {
            for path in &device.events {
                access.naga_nodes += 1;
                if self.open_node(path) {
                    access.readable_nodes += 1;
                }
            }
            if let Some(path) = &device.hidraw {
                if let Some(kind) = raw_kind(device) {
                    access.hidraw_nodes += 1;
                    if self.open_raw(path, kind) {
                        access.hidraw_readable += 1;
                    }
                }
            }
        }
        self.refresh_keyboards(&mut access);
        self.access = access;
        self.report_devices_from(&hid, true);
        self.apply_policy();
    }

    /// Opens a node when it is not open yet. Returns false when it cannot be
    /// read. Nodes without keys or pointer axes are read-checked and closed.
    fn open_node(&mut self, path: &PathBuf) -> bool {
        if self.nodes.values().any(|n| &n.device.path == path) {
            return true;
        }
        let Ok(device) = EvdevDevice::open(path) else { return false };
        let role = if device.caps.is_keyboard() {
            Role::Keyboard
        } else if device.caps.is_pointer() {
            Role::Pointer
        } else {
            return true;
        };
        let held = held_set(&device);
        let fd = device.fd();
        self.nodes.insert(fd, Node { device, role, grabbed: false, polled: false, frame: Vec::new(), state: NodeState { keys_down: held, ..Default::default() } });
        true
    }

    fn open_raw(&mut self, path: &PathBuf, kind: RawKind) -> bool {
        let source = path.to_string_lossy().to_string();
        if self.raws.values().any(|r| r.source == source) {
            return true;
        }
        match OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(path) {
            Ok(file) => {
                self.raws.insert(file.as_raw_fd(), RawNode { file, kind, source, polled: false });
                true
            }
            Err(_) => false,
        }
    }

    /// Other keyboards are opened read-only, without grabbing, only to see
    /// which modifiers are held when a mapped key is sent.
    fn refresh_keyboards(&mut self, access: &mut AccessCounts) {
        let mut keyboards = Vec::new();
        let Ok(entries) = std::fs::read_dir("/dev/input") else { return };
        let naga_paths: BTreeSet<PathBuf> = sysfs::naga_devices().into_iter().flat_map(|d| d.events).collect();
        for entry in entries.flatten() {
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.starts_with("event") || naga_paths.contains(&path) {
                continue;
            }
            let name = sysfs::event_name(&path).unwrap_or_default();
            if name == KEYBOARD_NAME || name == POINTER_NAME {
                continue;
            }
            if !is_keyboard_by_sysfs(&file_name) {
                continue;
            }
            access.keyboards_total += 1;
            if let Ok(file) = OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC).open(&path) {
                keyboards.push(file);
            }
        }
        self.out.keyboards = keyboards;
    }

    fn close_node(&mut self, fd: RawFd) {
        if let Some(mut node) = self.nodes.remove(&fd) {
            self.unwatch(fd);
            self.release_forwarded(&mut node);
            if node.grabbed {
                let _ = node.device.grab(false);
            }
        }
        if self.nodes.values().all(|n| !n.grabbed) {
            self.release_mapped();
        }
    }

    fn close_all(&mut self) {
        let fds: Vec<RawFd> = self.nodes.keys().copied().collect();
        for fd in fds {
            self.close_node(fd);
        }
        let raws: Vec<RawFd> = self.raws.keys().copied().collect();
        for fd in raws {
            self.unwatch(fd);
            self.raws.remove(&fd);
        }
        self.rm.driver_state.reset();
    }

    fn release_forwarded(&mut self, node: &mut Node) {
        for code in std::mem::take(&mut node.state.forwarded) {
            if ev::is_pointer_button(code) {
                self.out.button(code, false);
            } else {
                self.out.native_key(code, false, false);
            }
        }
        self.out.flush();
    }

    fn release_mapped(&mut self) {
        self.rm.release_mapped(&mut self.out);
    }

    // MARK: - Policy

    fn uses(&self, controls: &[i32]) -> bool {
        controls.iter().any(|c| self.standard.contains_key(c) || self.hypershift.contains_key(c))
    }

    fn needs_keyboard(&self) -> bool {
        self.rm.policy.remap && (self.uses(&(1..=12).collect::<Vec<_>>()) || self.uses(&[15, 16]) || self.rm.scroll.horizontal != 1.0)
            || (self.rm.policy.merge_native && !self.out.keyboards.is_empty())
    }

    fn needs_pointer(&self) -> bool {
        self.rm.policy.remap && (self.uses(&[15, 16, 17, 18, 19]) || self.rm.scroll != ScrollSensitivity::BASELINE)
    }

    fn apply_policy(&mut self) {
        let ready = self.out.is_ready() && !self.suspended;
        let keyboard = ready && self.needs_keyboard();
        let pointer = ready && self.needs_pointer();
        let fds: Vec<RawFd> = self.nodes.keys().copied().collect();
        for fd in fds {
            let (want_grab, want_poll) = {
                let node = &self.nodes[&fd];
                let grab = match node.role {
                    Role::Keyboard => keyboard,
                    Role::Pointer => pointer,
                };
                let poll = !self.suspended && (grab || node.role == Role::Keyboard || self.observe);
                (grab, poll)
            };
            self.set_polled(fd, want_poll);
            self.set_grab(fd, want_grab);
        }
        let raws: Vec<RawFd> = self.raws.keys().copied().collect();
        for fd in raws {
            let want = !self.suspended
                && match self.raws[&fd].kind {
                    RawKind::Vendor => true,
                    RawKind::Mouse => self.rm.driver_mode,
                };
            let raw = self.raws.get_mut(&fd).unwrap();
            if want != raw.polled {
                raw.polled = want;
                if want {
                    self.watch(fd, fd as u64);
                } else {
                    self.unwatch(fd);
                }
            }
        }
        self.publish_status();
    }

    fn set_polled(&mut self, fd: RawFd, on: bool) {
        let Some(node) = self.nodes.get_mut(&fd) else { return };
        if node.polled == on {
            return;
        }
        node.polled = on;
        if on {
            // Catch up with keys pressed while the node was not read.
            node.state.keys_down = held_set(&node.device);
            let mut discard = Vec::new();
            let _ = node.device.read_events(&mut discard);
            node.frame.clear();
            self.watch(fd, fd as u64);
        } else {
            self.unwatch(fd);
        }
    }

    /// Grabs once nothing is held, so the desktop never misses a release.
    fn set_grab(&mut self, fd: RawFd, on: bool) {
        let Some(node) = self.nodes.get(&fd) else { return };
        if node.grabbed == on {
            return;
        }
        if on {
            let held = node.device.held_keys();
            if (0..=ev::KEY_MAX).any(|c| evdev::is_held(&held, c)) {
                return; // Retried after the next event from this node.
            }
            if node.device.grab(true).is_ok() {
                self.nodes.get_mut(&fd).unwrap().grabbed = true;
            }
        } else {
            let mut node = self.nodes.remove(&fd).unwrap();
            self.release_forwarded(&mut node);
            let _ = node.device.grab(false);
            node.grabbed = false;
            self.nodes.insert(fd, node);
            if self.nodes.values().all(|n| !n.grabbed) {
                self.release_mapped();
            }
        }
    }

    fn publish_status(&mut self) {
        let status = EngineStatus {
            running: true,
            virtual_error: self.out.virtual_error.clone(),
            naga_nodes: self.access.naga_nodes,
            readable_nodes: self.access.readable_nodes,
            grabbed_nodes: self.nodes.values().filter(|n| n.grabbed).count(),
            hidraw_nodes: self.access.hidraw_nodes,
            hidraw_readable: self.access.hidraw_readable,
            keyboards_total: self.access.keyboards_total,
            keyboards_readable: self.out.keyboards.len(),
            remapping: self.rm.policy.remap && self.out.is_ready() && !self.suspended,
            suspended: self.suspended,
        };
        if status != self.status {
            self.status = status.clone();
            (self.emit)(Event::Status(status));
        }
    }

    fn report_devices(&mut self, force: bool) {
        let hid = sysfs::naga_devices();
        self.report_devices_from(&hid, force);
    }

    fn report_devices_from(&mut self, hid: &[HidDevice], force: bool) {
        let first = hid.first();
        let report = DeviceReport {
            name: first.map(|d| d.name.trim_start_matches("Razer ").to_string()),
            transport: first.map(|d| d.transport().to_string()),
            bluetooth_battery: sysfs::bluetooth_battery(hid),
        };
        if force || report != self.devices {
            self.devices = report.clone();
            (self.emit)(Event::Devices(report));
        }
    }

    fn publish_macros(&mut self) {
        if self.rm.mapper.status_changed {
            self.rm.mapper.status_changed = false;
            (self.emit)(Event::Macros { active: self.rm.mapper.active_macro_buttons().len(), error: self.rm.mapper.last_action_error.clone() });
        }
    }

    // MARK: - Input

    fn read_node(&mut self, fd: RawFd) {
        let mut events = Vec::with_capacity(64);
        let result = self.nodes[&fd].device.read_events(&mut events);
        for event in events {
            if event.type_ == ev::EV_SYN {
                if event.code == ev::SYN_REPORT {
                    self.handle_frame(fd);
                } else if event.code == ev::SYN_DROPPED {
                    // The kernel dropped events: resynchronize held keys.
                    if let Some(node) = self.nodes.get_mut(&fd) {
                        node.frame.clear();
                        node.state.keys_down = held_set(&node.device);
                    }
                }
            } else if let Some(node) = self.nodes.get_mut(&fd) {
                node.frame.push(event);
            }
        }
        if result.is_err() {
            // ENODEV: unplugged. The rescan cleans up the rest.
            self.close_node(fd);
            self.rescan_at = Some(Instant::now() + Duration::from_millis(100));
            return;
        }
        // A deferred grab waits for every key on this node to be released.
        let wants_grab = {
            let node = &self.nodes[&fd];
            !node.grabbed
                && node.state.keys_down.is_empty()
                && match node.role {
                    Role::Keyboard => self.out.is_ready() && !self.suspended && self.needs_keyboard(),
                    Role::Pointer => self.out.is_ready() && !self.suspended && self.needs_pointer(),
                }
        };
        if wants_grab {
            self.set_grab(fd, true);
            self.publish_status();
        }
    }

    fn handle_frame(&mut self, fd: RawFd) {
        let Some(node) = self.nodes.get_mut(&fd) else { return };
        let frame = std::mem::take(&mut node.frame);
        let mut notices = std::mem::take(&mut self.notices);
        self.rm.frame(&frame, node.grabbed, &mut node.state, &mut self.out, &mut notices);
        node.frame = frame;
        node.frame.clear();
        self.publish_notices(notices);
    }

    /// Hands notices to the UI and keeps the buffer for the next frame.
    fn publish_notices(&mut self, mut notices: Vec<Notice>) {
        for notice in notices.drain(..) {
            (self.emit)(match notice {
                Notice::Activity(c) => Event::Activity(c),
                Notice::LastInput(text) => Event::LastInput(text),
            });
        }
        self.notices = notices;
    }

    fn read_raw(&mut self, fd: RawFd) {
        let mut buffer = [0u8; 64];
        loop {
            let Some(raw) = self.raws.get_mut(&fd) else { return };
            match raw.file.read(&mut buffer) {
                Ok(0) => break,
                Ok(count) => {
                    let (kind, source) = (raw.kind, raw.source.clone());
                    self.handle_report(&buffer[..count], kind, &source);
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => {
                    self.unwatch(fd);
                    self.raws.remove(&fd);
                    self.rm.driver_state.reset();
                    self.rescan_at = Some(Instant::now() + Duration::from_millis(100));
                    break;
                }
            }
        }
        self.out.flush();
    }

    fn handle_report(&mut self, report: &[u8], kind: RawKind, source: &str) {
        let active = self.rm.policy.remap && self.out.is_ready() && !self.suspended;
        let mut notices = std::mem::take(&mut self.notices);
        self.rm.report(report, kind, source, active, &mut self.out, &mut notices);
        self.publish_notices(notices);
    }

    fn shutdown(&mut self) {
        self.release_mapped();
        self.close_all();
        self.out.release_everything();
        unsafe {
            libc::close(self.epoll);
            libc::close(self.inotify);
        }
    }
}

fn held_set(device: &EvdevDevice) -> BTreeSet<u16> {
    let held = device.held_keys();
    (0..=ev::KEY_MAX).filter(|c| evdev::is_held(&held, *c)).collect()
}

fn raw_kind(device: &HidDevice) -> Option<RawKind> {
    if device.vendor != sysfs::RAZER_VENDOR || device.product != sysfs::RECEIVER_PRODUCT {
        return None;
    }
    if device.descriptor.input_bits.get(&4) == Some(&120) {
        Some(RawKind::Vendor)
    } else if device.descriptor.primary_usage() == Some((1, 2)) && !device.descriptor.uses_report_ids {
        Some(RawKind::Mouse)
    } else {
        None
    }
}

/// udev marks keyboards with ID_INPUT_KEYBOARD; read it from the database
/// without opening the device.
fn is_keyboard_by_sysfs(event: &str) -> bool {
    let Ok(dev) = std::fs::read_to_string(format!("/sys/class/input/{event}/dev")) else { return false };
    std::fs::read_to_string(format!("/run/udev/data/c{}", dev.trim()))
        .map(|data| data.lines().any(|l| l == "E:ID_INPUT_KEYBOARD=1"))
        .unwrap_or(false)
}
