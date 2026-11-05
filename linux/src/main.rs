//! OpenNaga for Linux: remaps the buttons of the Razer Naga V2 HyperSpeed
//! and controls DPI, polling rate and mouse memory over its USB receiver.

mod desktop;
mod diagnostics;
mod hw;
mod input;
mod model;
mod ui;

pub const APP_ID: &str = "io.github.zer0codestuff.OpenNaga";
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const HELP: &str = "OpenNaga for Linux: Razer Naga V2 HyperSpeed button remapping

Usage: opennaga [option]

  (no option)                Open the settings window
  --background               Start in the background (used at sign-in)
  --show-settings            Open the settings window
  --diagnose                 Read hardware values over USB and print JSON
  --diagnose-file <path>     Write the report to a file
  --inspect-onboard          Also dump the 21 stored and active assignments
  --verify-hardware          Write the current DPI and polling rate back and verify
  --save-onboard-profile     Save the selected profile to mouse memory
  --restore-onboard-profile  Restore the assignments saved before
  --probe-input [seconds]    Print decoded mouse input without capturing it
  --print-udev-rule          Print the access rule for /etc/udev/rules.d
  --snapshot <png>           Render the window to a PNG and quit
      --snapshot-appearance light|dark, --snapshot-button <1-19>,
      --snapshot-section <name>, --snapshot-hypershift, --snapshot-size WxH
  --version                  Print the version
";

fn value_after(args: &[String], flag: &str) -> Option<String> {
    args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    if has("--help") || has("-h") {
        print!("{HELP}");
        return;
    }
    if has("--version") {
        println!("OpenNaga {VERSION}");
        return;
    }
    if has("--print-udev-rule") {
        print!("{}", desktop::access::RULE);
        return;
    }
    if has("--probe-input") {
        let seconds = value_after(&args, "--probe-input").and_then(|v| v.parse().ok()).unwrap_or(15);
        std::process::exit(diagnostics::probe_input(seconds));
    }
    let hardware_flags = ["--diagnose", "--diagnose-file", "--verify-hardware", "--inspect-onboard", "--save-onboard-profile", "--restore-onboard-profile"];
    if hardware_flags.iter().any(|f| has(f)) {
        let request = diagnostics::Request {
            verify_writes: has("--verify-hardware"),
            inspect_onboard: has("--inspect-onboard"),
            save_onboard: has("--save-onboard-profile"),
            restore_onboard: has("--restore-onboard-profile"),
            output: value_after(&args, "--diagnose-file").map(Into::into),
        };
        std::process::exit(diagnostics::run(request));
    }
    let options = ui::Options {
        background: has("--background"),
        show_settings: has("--show-settings"),
        snapshot: value_after(&args, "--snapshot"),
        snapshot_dark: value_after(&args, "--snapshot-appearance").map(|v| v == "dark"),
        snapshot_button: value_after(&args, "--snapshot-button").and_then(|v| v.parse().ok()),
        snapshot_section: value_after(&args, "--snapshot-section"),
        snapshot_hypershift: has("--snapshot-hypershift"),
        snapshot_size: value_after(&args, "--snapshot-size").and_then(|v| {
            let (w, h) = v.split_once('x')?;
            Some((w.parse().ok()?, h.parse().ok()?))
        }),
    };
    if options.snapshot.is_none() {
        // Before any thread starts, so only the signal thread receives them.
        ui::block_signals();
    }
    let code = ui::run(options);
    std::process::exit(code.get() as i32);
}
