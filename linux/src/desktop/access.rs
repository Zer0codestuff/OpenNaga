//! Device access. Remapping needs read access to the Naga's input nodes and
//! write access to /dev/uinput; DPI, polling rate and mouse memory need the
//! receiver's hidraw node. The udev rule grants these to the user signed in
//! at the seat (systemd-logind `uaccess`), only for this mouse.

use std::path::PathBuf;

use std::io::Write;
use std::process::{Command, Stdio};

/// Where the Status pane installs the rule. Packages use /usr/lib instead.
pub const RULE_FILE: &str = "/etc/udev/rules.d/60-opennaga.rules";
const RULE_LOCATIONS: [&str; 3] = [RULE_FILE, "/usr/local/lib/udev/rules.d/60-opennaga.rules", "/usr/lib/udev/rules.d/60-opennaga.rules"];

pub const RULE: &str = include_str!("../../data/60-opennaga.rules");

/// True when udev uses the current rule. A file in /etc hides the
/// same name in /usr/lib, so the first one found is the one that counts.
pub fn rule_installed() -> bool {
    RULE_LOCATIONS
        .iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .map(|text| text.trim() == RULE.trim())
        .unwrap_or(false)
}

/// Writes the rule with pkexec, reloads udev and re-applies it to devices
/// that are already connected. Blocks until the password prompt closes.
/// The rule travels on standard input, so no file outside root's control
/// is ever copied into /etc.
pub fn install_rule() -> Result<(), String> {
    let script = format!(
        "umask 022 && cat > '{RULE_FILE}.new' && mv -f '{RULE_FILE}.new' '{RULE_FILE}' && udevadm control --reload-rules && \
         udevadm trigger --action=change --subsystem-match=input --subsystem-match=hidraw --subsystem-match=misc"
    );
    let mut child = Command::new("pkexec")
        .args(["/bin/sh", "-c", &script])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| format!("pkexec is not available: {e}. Install the rule manually from the README."))?;
    if let Some(mut stdin) = child.stdin.take() {
        // A refused prompt closes the pipe; the exit status explains why.
        let _ = stdin.write_all(RULE.as_bytes());
    }
    match child.wait() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) if s.code() == Some(126) || s.code() == Some(127) => Err("The administrator password was not entered.".into()),
        Ok(s) => Err(format!("Installing the rule failed (exit status {}).", s.code().unwrap_or(-1))),
        Err(e) => Err(e.to_string()),
    }
}

pub fn autostart_path() -> PathBuf {
    let config = std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from).unwrap_or_else(|| crate::model::data_folder::home().join(".config"));
    config.join("autostart").join(format!("{}.desktop", crate::APP_ID))
}

pub fn autostart_enabled() -> bool {
    autostart_path().exists()
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let path = autostart_path();
    if !enabled {
        return match std::fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        };
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let text = format!(
        "[Desktop Entry]\nType=Application\nName=OpenNaga\nComment=Razer Naga V2 HyperSpeed button remapping\nExec={} --background\nIcon={}\nX-GNOME-Autostart-enabled=true\nNoDisplay=true\n",
        quote_exec(&exe.to_string_lossy()),
        crate::APP_ID
    );
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    std::fs::write(&path, text).map_err(|e| e.to_string())
}

/// Quotes a path for a desktop entry Exec key (Desktop Entry Specification:
/// double quotes, with `"`, `` ` ``, `$` and `\` escaped by a backslash).
/// Backslashes are doubled again because the value is itself an escaped string.
fn quote_exec(path: &str) -> String {
    let mut quoted = String::from("\"");
    for c in path.chars() {
        match c {
            '"' | '`' | '$' => {
                quoted.push_str("\\\\");
                quoted.push(c);
            }
            '\\' => quoted.push_str("\\\\\\\\"),
            '%' => quoted.push_str("%%"),
            _ => quoted.push(c),
        }
    }
    quoted.push('"');
    quoted
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exec_paths_are_quoted() {
        assert_eq!(quote_exec("/usr/bin/opennaga"), "\"/usr/bin/opennaga\"");
        assert_eq!(quote_exec("/home/a b/$x"), "\"/home/a b/\\\\$x\"");
        assert_eq!(quote_exec("/opt/100%"), "\"/opt/100%%\"");
    }

    #[test]
    fn rule_covers_the_receiver_and_uinput() {
        assert!(RULE.contains("ATTRS{idProduct}==\"00b4\""));
        assert!(RULE.contains("KERNEL==\"uinput\""));
        assert!(!RULE.contains("MODE=\"0666\""));
    }
}
