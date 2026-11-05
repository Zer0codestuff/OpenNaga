//! Command-line hardware checks, the counterpart of the macOS diagnostics.
//! Reading is the default; writes happen only with the explicit flags.

use crate::desktop::system::Environment;
use crate::hw::hidraw::HidrawTransport;
use crate::hw::onboard::{self, OnboardProfilePlan, PlanContext};
use crate::hw::protocol::{RazerError, RazerTransport, Session};
use crate::hw::sysfs;
use crate::input::decode;
use crate::input::evdev::EvdevDevice;
use crate::model::actions::MappingLayer;
use crate::model::config::ConfigManager;
use crate::model::data_folder;
use crate::model::keys::ev;
use crate::model::layout::LayoutMap;
use crate::model::prefs::Prefs;
use serde_json::{json, Map, Value};
use std::path::PathBuf;

#[derive(Default)]
pub struct Request {
    pub verify_writes: bool,
    pub inspect_onboard: bool,
    pub save_onboard: bool,
    pub restore_onboard: bool,
    pub output: Option<PathBuf>,
}

pub fn run(request: Request) -> i32 {
    let mut report = Map::new();
    report.insert("device".into(), json!("Razer Naga V2 HyperSpeed"));
    report.insert("receiver".into(), json!("1532:00b4"));
    report.insert("writesRequested".into(), json!(request.verify_writes || request.save_onboard || request.restore_onboard));
    let devices = sysfs::naga_devices();
    report.insert(
        "hidDevices".into(),
        Value::Array(
            devices
                .iter()
                .map(|d| {
                    json!({
                        "kernel": d.kernel_name, "name": d.name, "transport": d.transport(), "phys": d.phys,
                        "hidraw": d.hidraw.as_ref().map(|p| p.display().to_string()),
                        "events": d.events.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
                        "primaryUsage": d.descriptor.primary_usage().map(|(p, u)| format!("{p:02x}:{u:02x}")),
                        "featureBytes": d.descriptor.feature_bytes(0),
                    })
                })
                .collect(),
        ),
    );
    report.insert("accessRuleInstalled".into(), json!(crate::desktop::access::rule_installed()));
    report.insert("uinputWritable".into(), json!(std::fs::OpenOptions::new().write(true).open("/dev/uinput").is_ok()));
    let mut status = 0;
    if let Err(error) = hardware(&request, &mut report) {
        report.insert("error".into(), json!(error.to_string()));
        status = 1;
    }
    let text = serde_json::to_string_pretty(&Value::Object(report)).unwrap_or_default();
    match &request.output {
        Some(path) => {
            if let Err(error) = std::fs::write(path, text + "\n") {
                eprintln!("Diagnostic output failed: {error}");
                return 1;
            }
        }
        None => println!("{text}"),
    }
    status
}

fn hardware(request: &Request, report: &mut Map<String, Value>) -> Result<(), RazerError> {
    if request.save_onboard && request.restore_onboard {
        return Err(RazerError::InvalidValue("Choose either save or restore.".into()));
    }
    let mut transport = HidrawTransport::open()?;
    let identity = transport.identity();
    report.insert("hidraw".into(), json!(transport.path.display().to_string()));
    let mut session = Session::new(&mut transport as &mut dyn RazerTransport);
    let snapshot = session.read_snapshot()?;
    report.insert("dpiX".into(), json!(snapshot.dpi_x));
    report.insert("dpiY".into(), json!(snapshot.dpi_y));
    report.insert("pollingRate".into(), json!(snapshot.polling_rate));
    report.insert("batteryPercent".into(), json!(snapshot.battery_level));
    report.insert("mode".into(), json!(snapshot.mode));
    report.insert("readVerified".into(), json!(true));
    report.insert("warnings".into(), json!(snapshot.warnings));
    let journal = onboard::journal_path();
    if request.save_onboard {
        let mut config = ConfigManager::new(data_folder::file("profiles.json"), Prefs::load(), false);
        config.load();
        if config.profiles_read_only {
            return Err(RazerError::InvalidValue("Reload or restore the local profiles file before saving to the mouse.".into()));
        }
        let layout = LayoutMap::us();
        let env = Environment::shared();
        let system = |action| env.plan(action);
        let context = PlanContext { layout: &layout, system: &system };
        let plan = OnboardProfilePlan::new(
            &config.current_profile_name,
            &config.mapping_for_current(MappingLayer::Standard),
            &config.mapping_for_current(MappingLayer::Hypershift),
            &context,
        );
        report.insert("profile".into(), json!(plan.name));
        report.insert("planIssues".into(), json!(plan.issues));
        onboard::save(&plan, &mut session, &identity, &journal)?;
        report.insert("onboardSaveVerified".into(), json!(true));
    }
    if request.restore_onboard {
        onboard::restore(&mut session, &identity, &journal)?;
        report.insert("onboardRestoreVerified".into(), json!(true));
    }
    if request.inspect_onboard || request.save_onboard || request.restore_onboard {
        let bytes = |list: Vec<onboard::Binding>| list.into_iter().map(|b| b.bytes).collect::<Vec<_>>();
        report.insert("onboardActive".into(), json!(bytes(onboard::read_bindings(&mut session, 0, 0)?)));
        report.insert("onboardStored".into(), json!(bytes(onboard::read_bindings(&mut session, 1, 0)?)));
        report.insert("onboardReadVerified".into(), json!(true));
    }
    if request.verify_writes {
        let (Some(x), Some(y), Some(rate)) = (snapshot.dpi_x, snapshot.dpi_y, snapshot.polling_rate) else {
            return Err(RazerError::InvalidValue("Read DPI and polling rate before verifying writes.".into()));
        };
        // Exercise both setters without changing the user's sensitivity.
        session.set_dpi(x, y)?;
        report.insert("dpiWriteReadbackVerified".into(), json!(true));
        session.set_polling(rate)?;
        report.insert("pollingWriteReadbackVerified".into(), json!(true));
    }
    Ok(())
}

/// Prints decoded Naga input without grabbing it. Read-only.
pub fn probe_input(seconds: u64) -> i32 {
    let devices = sysfs::naga_devices();
    let mut open = Vec::new();
    for path in devices.iter().flat_map(|d| d.events.iter()) {
        match EvdevDevice::open(path) {
            Ok(device) => {
                println!("Watching {} ({})", path.display(), device.name);
                open.push(device);
            }
            Err(error) => println!("Cannot read {}: {error}", path.display()),
        }
    }
    if open.is_empty() {
        println!("No readable Naga input nodes. Install the access rule (opennaga --print-udev-rule).");
        return 1;
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(seconds);
    let mut fds: Vec<libc::pollfd> = open.iter().map(|d| libc::pollfd { fd: d.fd(), events: libc::POLLIN, revents: 0 }).collect();
    while std::time::Instant::now() < deadline {
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 200) };
        if ready <= 0 {
            continue;
        }
        for (index, pollfd) in fds.iter().enumerate() {
            if pollfd.revents & libc::POLLIN == 0 {
                continue;
            }
            let mut events = Vec::new();
            let _ = open[index].read_events(&mut events);
            for event in events {
                match event.type_ {
                    ev::EV_KEY if event.value != 2 => {
                        let control = decode::control_for_key(event.code, false).map(|c| format!(" = control {c}")).unwrap_or_default();
                        println!("{}: key {} {}{control}", open[index].name, event.code, if event.value == 1 { "down" } else { "up" });
                    }
                    ev::EV_REL if matches!(event.code, ev::REL_WHEEL | ev::REL_HWHEEL) => {
                        println!("{}: {} {}", open[index].name, if event.code == ev::REL_WHEEL { "wheel" } else { "tilt" }, event.value);
                    }
                    _ => {}
                }
            }
        }
    }
    0
}
