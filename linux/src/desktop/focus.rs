//! Follows the focused window so per-app profiles can apply. The app
//! identifier is the window class (Wayland app_id), the same name desktop
//! files use as StartupWMClass. Wayland has no common protocol for this, so
//! each compositor family has its own backend: Hyprland's event socket,
//! Sway and i3 IPC, and X11 through xprop.

use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Focus {
    pub app_id: String,
    pub title: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Backend {
    Hyprland,
    Sway,
    X11,
    Unsupported,
}

impl Backend {
    pub fn detect() -> Backend {
        if hyprland_dir().is_some() {
            Backend::Hyprland
        } else if sway_socket().is_some() {
            Backend::Sway
        } else if std::env::var_os("WAYLAND_DISPLAY").is_none() && std::env::var_os("DISPLAY").is_some() && crate::input::typing::which("xprop") {
            Backend::X11
        } else {
            Backend::Unsupported
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Backend::Hyprland => "Hyprland",
            Backend::Sway => "Sway or i3",
            Backend::X11 => "X11",
            Backend::Unsupported => "unsupported",
        }
    }
}

fn hyprland_dir() -> Option<PathBuf> {
    let signature = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty())?;
    let runtime = std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into());
    [PathBuf::from(&runtime).join("hypr").join(&signature), PathBuf::from("/tmp/hypr").join(&signature)]
        .into_iter()
        .find(|p| p.join(".socket2.sock").exists())
}

fn sway_socket() -> Option<PathBuf> {
    ["SWAYSOCK", "I3SOCK"].iter().filter_map(std::env::var_os).map(PathBuf::from).find(|p| p.exists())
}

/// Starts watching. `deliver` runs on the watcher thread and must not block.
pub fn spawn(deliver: impl Fn(Focus) + Send + Sync + 'static) -> Backend {
    let backend = Backend::detect();
    let deliver: Arc<dyn Fn(Focus) + Send + Sync> = Arc::new(deliver);
    let run: Option<Box<dyn FnOnce() + Send>> = match backend {
        Backend::Hyprland => Some(Box::new(move || loop {
            if let Err(error) = watch_hyprland(&deliver) {
                eprintln!("[Focus] Hyprland event socket: {error}");
            }
            std::thread::sleep(Duration::from_secs(2));
        })),
        Backend::Sway => Some(Box::new(move || loop {
            if let Err(error) = watch_sway(&deliver) {
                eprintln!("[Focus] Sway IPC: {error}");
            }
            std::thread::sleep(Duration::from_secs(2));
        })),
        Backend::X11 => Some(Box::new(move || loop {
            if let Err(error) = watch_x11(&deliver) {
                eprintln!("[Focus] xprop: {error}");
            }
            std::thread::sleep(Duration::from_secs(2));
        })),
        Backend::Unsupported => None,
    };
    if let Some(run) = run {
        let _ = std::thread::Builder::new().name("Focus watcher".into()).spawn(run);
    }
    backend
}

// MARK: - Hyprland

fn hyprland_request(command: &str) -> std::io::Result<String> {
    let dir = hyprland_dir().ok_or_else(|| std::io::Error::other("no Hyprland socket"))?;
    let mut stream = UnixStream::connect(dir.join(".socket.sock"))?;
    stream.set_read_timeout(Some(Duration::from_secs(2)))?;
    stream.write_all(command.as_bytes())?;
    let mut reply = String::new();
    stream.read_to_string(&mut reply)?;
    Ok(reply)
}

fn watch_hyprland(deliver: &Arc<dyn Fn(Focus) + Send + Sync>) -> std::io::Result<()> {
    if let Ok(reply) = hyprland_request("j/activewindow") {
        if let Some(focus) = parse_hyprland_active(&reply) {
            deliver(focus);
        }
    }
    let dir = hyprland_dir().ok_or_else(|| std::io::Error::other("no Hyprland socket"))?;
    let stream = UnixStream::connect(dir.join(".socket2.sock"))?;
    for line in BufReader::new(stream).lines() {
        if let Some(focus) = parse_hyprland_event(&line?) {
            deliver(focus);
        }
    }
    Ok(())
}

pub fn parse_hyprland_event(line: &str) -> Option<Focus> {
    let payload = line.strip_prefix("activewindow>>")?;
    let (class, title) = payload.split_once(',').unwrap_or((payload, ""));
    Some(Focus { app_id: class.to_string(), title: title.to_string() })
}

pub fn parse_hyprland_active(json: &str) -> Option<Focus> {
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let class = value.get("class")?.as_str()?.to_string();
    Some(Focus { app_id: class, title: value.get("title").and_then(|t| t.as_str()).unwrap_or("").to_string() })
}

// MARK: - Sway and i3

fn i3_message(stream: &mut UnixStream, kind: u32, payload: &str) -> std::io::Result<()> {
    let mut message = b"i3-ipc".to_vec();
    message.extend((payload.len() as u32).to_ne_bytes());
    message.extend(kind.to_ne_bytes());
    message.extend(payload.as_bytes());
    stream.write_all(&message)
}

fn i3_read(stream: &mut UnixStream) -> std::io::Result<(u32, String)> {
    let mut header = [0u8; 14];
    stream.read_exact(&mut header)?;
    if &header[..6] != b"i3-ipc" {
        return Err(std::io::Error::other("bad IPC header"));
    }
    let length = u32::from_ne_bytes(header[6..10].try_into().unwrap()) as usize;
    let kind = u32::from_ne_bytes(header[10..14].try_into().unwrap());
    let mut body = vec![0u8; length];
    stream.read_exact(&mut body)?;
    Ok((kind, String::from_utf8_lossy(&body).to_string()))
}

fn i3_tree() -> std::io::Result<serde_json::Value> {
    let path = sway_socket().ok_or_else(|| std::io::Error::other("no IPC socket"))?;
    let mut stream = UnixStream::connect(path)?;
    i3_message(&mut stream, 4, "")?;
    let (_, body) = i3_read(&mut stream)?;
    serde_json::from_str(&body).map_err(std::io::Error::other)
}

fn watch_sway(deliver: &Arc<dyn Fn(Focus) + Send + Sync>) -> std::io::Result<()> {
    if let Ok(tree) = i3_tree() {
        if let Some(focus) = focused_in_tree(&tree) {
            deliver(focus);
        }
    }
    let path = sway_socket().ok_or_else(|| std::io::Error::other("no IPC socket"))?;
    let mut stream = UnixStream::connect(path)?;
    i3_message(&mut stream, 2, r#"["window"]"#)?;
    loop {
        let (kind, body) = i3_read(&mut stream)?;
        if kind & 0x8000_0000 == 0 {
            continue;
        }
        if let Some(focus) = parse_i3_window_event(&body) {
            deliver(focus);
        }
    }
}

fn node_focus(node: &serde_json::Value) -> Option<Focus> {
    let app_id = node
        .get("app_id")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .or_else(|| node.get("window_properties").and_then(|p| p.get("class")).and_then(|v| v.as_str()))?;
    Some(Focus { app_id: app_id.to_string(), title: node.get("name").and_then(|v| v.as_str()).unwrap_or("").to_string() })
}

pub fn parse_i3_window_event(body: &str) -> Option<Focus> {
    let value: serde_json::Value = serde_json::from_str(body).ok()?;
    if value.get("change")?.as_str()? != "focus" {
        return None;
    }
    node_focus(value.get("container")?)
}

pub fn focused_in_tree(node: &serde_json::Value) -> Option<Focus> {
    if node.get("focused").and_then(|v| v.as_bool()) == Some(true) {
        if let Some(focus) = node_focus(node) {
            return Some(focus);
        }
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(|v| v.as_array()) {
            for child in children {
                if let Some(focus) = focused_in_tree(child) {
                    return Some(focus);
                }
            }
        }
    }
    None
}

fn collect_tree(node: &serde_json::Value, into: &mut Vec<String>) {
    if let Some(focus) = node_focus(node) {
        into.push(focus.app_id);
    }
    for key in ["nodes", "floating_nodes"] {
        if let Some(children) = node.get(key).and_then(|v| v.as_array()) {
            for child in children {
                collect_tree(child, into);
            }
        }
    }
}

// MARK: - X11

fn watch_x11(deliver: &Arc<dyn Fn(Focus) + Send + Sync>) -> std::io::Result<()> {
    let mut child = Command::new("xprop").args(["-root", "-spy", "_NET_ACTIVE_WINDOW"]).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let stdout = child.stdout.take().ok_or_else(|| std::io::Error::other("no output"))?;
    for line in BufReader::new(stdout).lines() {
        let Some(id) = line?.rsplit(' ').next().map(str::to_string) else { continue };
        if id == "0x0" {
            continue;
        }
        let output = Command::new("xprop").args(["-id", &id, "WM_CLASS", "_NET_WM_NAME"]).output()?;
        if let Some(focus) = parse_xprop(&String::from_utf8_lossy(&output.stdout)) {
            deliver(focus);
        }
    }
    let _ = child.wait();
    Ok(())
}

pub fn parse_xprop(text: &str) -> Option<Focus> {
    let mut class = None;
    let mut title = String::new();
    for line in text.lines() {
        if line.starts_with("WM_CLASS") {
            let parts: Vec<&str> = line.split('"').collect();
            // WM_CLASS(STRING) = "instance", "Class"
            class = parts.get(3).or_else(|| parts.get(1)).map(|s| s.to_string());
        } else if line.starts_with("_NET_WM_NAME") {
            title = line.split_once(" = ").map(|(_, v)| v.trim_matches('"').to_string()).unwrap_or_default();
        }
    }
    Some(Focus { app_id: class?, title })
}

/// App identifiers of windows open right now, for the app picker.
pub fn running_app_ids() -> Vec<String> {
    let mut ids = Vec::new();
    match Backend::detect() {
        Backend::Hyprland => {
            if let Ok(reply) = hyprland_request("j/clients") {
                if let Ok(serde_json::Value::Array(clients)) = serde_json::from_str::<serde_json::Value>(&reply) {
                    ids.extend(clients.iter().filter_map(|c| c.get("class").and_then(|v| v.as_str()).map(str::to_string)));
                }
            }
        }
        Backend::Sway => {
            if let Ok(tree) = i3_tree() {
                collect_tree(&tree, &mut ids);
            }
        }
        _ => {}
    }
    ids.retain(|id| !id.is_empty());
    ids.sort();
    ids.dedup();
    ids
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprland_events() {
        assert_eq!(parse_hyprland_event("activewindow>>firefox,Mozilla, Firefox"), Some(Focus { app_id: "firefox".into(), title: "Mozilla, Firefox".into() }));
        assert_eq!(parse_hyprland_event("activewindow>>,"), Some(Focus { app_id: String::new(), title: String::new() }));
        assert_eq!(parse_hyprland_event("workspace>>2"), None);
        assert_eq!(parse_hyprland_active(r#"{"class":"steam","title":"Steam"}"#).unwrap().app_id, "steam");
        assert_eq!(parse_hyprland_active("{}"), None);
    }

    #[test]
    fn i3_events_and_tree() {
        let event = r#"{"change":"focus","container":{"app_id":null,"name":"Doc","window_properties":{"class":"LibreOffice"}}}"#;
        assert_eq!(parse_i3_window_event(event).unwrap().app_id, "LibreOffice");
        assert_eq!(parse_i3_window_event(r#"{"change":"title","container":{"app_id":"foot"}}"#), None);
        let tree: serde_json::Value = serde_json::from_str(
            r#"{"nodes":[{"nodes":[{"app_id":"foot","focused":false},{"app_id":"org.gnome.Nautilus","focused":true,"name":"Home"}]}]}"#,
        )
        .unwrap();
        assert_eq!(focused_in_tree(&tree).unwrap().app_id, "org.gnome.Nautilus");
        let mut all = Vec::new();
        collect_tree(&tree, &mut all);
        assert_eq!(all, vec!["foot", "org.gnome.Nautilus"]);
    }

    #[test]
    fn xprop_output() {
        let text = "WM_CLASS(STRING) = \"navigator\", \"firefox\"\n_NET_WM_NAME(UTF8_STRING) = \"Page\"\n";
        assert_eq!(parse_xprop(text), Some(Focus { app_id: "firefox".into(), title: "Page".into() }));
        assert_eq!(parse_xprop("WM_CLASS:  not found.\n"), None);
    }
}
