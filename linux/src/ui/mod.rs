//! GTK4 and libadwaita user interface.

pub mod buttons;
pub mod dialogs;
pub mod inspector;
pub mod keymap;
pub mod panes;
pub mod state;
pub mod tray;
pub mod widgets;
pub mod window;

use crate::model::actions::MappingLayer;
use adw::prelude::*;
use gtk::{gdk, gdk_pixbuf, gio, glib};
use state::{App, Message, Section};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

const CSS: &str = include_str!("style.css");
const ICON: &[u8] = include_bytes!("../../data/icons/opennaga-256.png");

#[derive(Clone, Debug, Default)]
pub struct Options {
    pub background: bool,
    pub snapshot: Option<String>,
    pub snapshot_dark: Option<bool>,
    pub snapshot_button: Option<i32>,
    pub snapshot_section: Option<String>,
    pub snapshot_hypershift: bool,
    pub snapshot_size: Option<(i32, i32)>,
    pub show_settings: bool,
}

fn accent_css(dark: bool) -> String {
    let (accent, background) = if dark { ("#7ACF66", "#3C8C46") } else { ("#297A3D", "#2E8540") };
    format!(
        "@define-color naga_accent {accent};\n\
         @define-color accent_color {accent};\n@define-color accent_bg_color {background};\n@define-color accent_fg_color #ffffff;\n\
         :root {{ --accent-color: {accent}; --accent-bg-color: {background}; --accent-fg-color: #ffffff; }}\n"
    )
}

const SYMBOLIC: [(&str, &str); 22] = [
    ("opennaga-allow-symbolic", include_str!("../../data/symbolic/opennaga-allow-symbolic.svg")),
    ("opennaga-app-symbolic", include_str!("../../data/symbolic/opennaga-app-symbolic.svg")),
    ("opennaga-apps-symbolic", include_str!("../../data/symbolic/opennaga-apps-symbolic.svg")),
    ("opennaga-background-symbolic", include_str!("../../data/symbolic/opennaga-background-symbolic.svg")),
    ("opennaga-buttons-symbolic", include_str!("../../data/symbolic/opennaga-buttons-symbolic.svg")),
    ("opennaga-check-symbolic", include_str!("../../data/symbolic/opennaga-check-symbolic.svg")),
    ("opennaga-disabled-symbolic", include_str!("../../data/symbolic/opennaga-disabled-symbolic.svg")),
    ("opennaga-down-symbolic", include_str!("../../data/symbolic/opennaga-down-symbolic.svg")),
    ("opennaga-memory-symbolic", include_str!("../../data/symbolic/opennaga-memory-symbolic.svg")),
    ("opennaga-more-symbolic", include_str!("../../data/symbolic/opennaga-more-symbolic.svg")),
    ("opennaga-mouse-filled-symbolic", include_str!("../../data/symbolic/opennaga-mouse-filled-symbolic.svg")),
    ("opennaga-mouse-symbolic", include_str!("../../data/symbolic/opennaga-mouse-symbolic.svg")),
    ("opennaga-original-symbolic", include_str!("../../data/symbolic/opennaga-original-symbolic.svg")),
    ("opennaga-play-symbolic", include_str!("../../data/symbolic/opennaga-play-symbolic.svg")),
    ("opennaga-refresh-symbolic", include_str!("../../data/symbolic/opennaga-refresh-symbolic.svg")),
    ("opennaga-remove-symbolic", include_str!("../../data/symbolic/opennaga-remove-symbolic.svg")),
    ("opennaga-sensitivity-symbolic", include_str!("../../data/symbolic/opennaga-sensitivity-symbolic.svg")),
    ("opennaga-shift-symbolic", include_str!("../../data/symbolic/opennaga-shift-symbolic.svg")),
    ("opennaga-status-symbolic", include_str!("../../data/symbolic/opennaga-status-symbolic.svg")),
    ("opennaga-up-symbolic", include_str!("../../data/symbolic/opennaga-up-symbolic.svg")),
    ("opennaga-usb-symbolic", include_str!("../../data/symbolic/opennaga-usb-symbolic.svg")),
    ("opennaga-warning-symbolic", include_str!("../../data/symbolic/opennaga-warning-symbolic.svg")),
];

/// OpenNaga's own symbolic icons, so the window looks the same with any
/// icon theme, plus the app icon for uninstalled runs. They are written to
/// the cache folder and registered there.
fn install_icons(display: &gdk::Display) {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| crate::model::data_folder::home().join(".cache"));
    let dir = base.join("opennaga").join("icons");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    const APP_ICON: &str = include_str!("../../data/icons/io.github.zer0codestuff.OpenNaga.svg");
    let app_icon = (crate::APP_ID, APP_ICON);
    for &(name, svg) in SYMBOLIC.iter().chain(std::iter::once(&app_icon)) {
        let path = dir.join(format!("{name}.svg"));
        if std::fs::read_to_string(&path).map(|existing| existing != svg).unwrap_or(true) {
            let _ = std::fs::write(&path, svg);
        }
    }
    gtk::IconTheme::for_display(display).add_search_path(&dir);
}

fn install_style(display: &gdk::Display) {
    install_icons(display);
    let base = gtk::CssProvider::new();
    base.load_from_string(CSS);
    gtk::style_context_add_provider_for_display(display, &base, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    let accent = gtk::CssProvider::new();
    gtk::style_context_add_provider_for_display(display, &accent, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
    let manager = adw::StyleManager::default();
    accent.load_from_string(&accent_css(manager.is_dark()));
    manager.connect_dark_notify(move |m| accent.load_from_string(&accent_css(m.is_dark())));
}

fn tray_icons() -> Vec<ksni::Icon> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(ICON));
    let Ok(pixbuf) = gdk_pixbuf::Pixbuf::from_stream(&stream, gio::Cancellable::NONE) else { return vec![] };
    [22, 32, 48, 64]
        .iter()
        .filter_map(|size| {
            let scaled = pixbuf.scale_simple(*size, *size, gdk_pixbuf::InterpType::Hyper)?.add_alpha(false, 0, 0, 0).ok()?;
            let stride = scaled.rowstride() as usize;
            let bytes = scaled.read_pixel_bytes();
            let mut rgba = Vec::with_capacity((size * size * 4) as usize);
            for y in 0..*size as usize {
                rgba.extend_from_slice(&bytes[y * stride..y * stride + *size as usize * 4]);
            }
            Some(tray::icon_from_rgba(*size, *size, &rgba))
        })
        .collect()
}

#[allow(deprecated)]
fn watch_sleep(app: &Rc<App>) {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE) else { return };
    let weak = Rc::downgrade(app);
    let id = bus.signal_subscribe(
        Some("org.freedesktop.login1"),
        Some("org.freedesktop.login1.Manager"),
        Some("PrepareForSleep"),
        Some("/org/freedesktop/login1"),
        None,
        gio::DBusSignalFlags::NONE,
        move |_, _, _, _, _, parameters| {
            if let (Some(app), Some((sleeping,))) = (weak.upgrade(), parameters.get::<(bool,)>()) {
                app.suspend(sleeping);
            }
        },
    );
    // The subscription lives as long as the app.
    std::mem::forget((bus, id));
}

/// SIGTERM, SIGINT and SIGHUP quit through the same path as the Quit item,
/// so a changed driver mode is restored. The signals were blocked in main
/// before any thread started; this thread waits for them.
fn watch_signals(sender: async_channel::Sender<Message>) {
    std::thread::Builder::new()
        .name("Signals".into())
        .spawn(move || unsafe {
            let mut set: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut set);
            for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
                libc::sigaddset(&mut set, signal);
            }
            loop {
                let mut signal = 0;
                if libc::sigwait(&set, &mut signal) == 0 {
                    let _ = sender.send_blocking(Message::Quit);
                }
            }
        })
        .ok();
}

pub fn block_signals() {
    unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for signal in [libc::SIGTERM, libc::SIGINT, libc::SIGHUP] {
            libc::sigaddset(&mut set, signal);
        }
        libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
    }
}

pub fn run(options: Options) -> glib::ExitCode {
    let snapshot = options.snapshot.is_some();
    let mut flags = gio::ApplicationFlags::empty();
    if snapshot {
        flags |= gio::ApplicationFlags::NON_UNIQUE;
    }
    let application = adw::Application::builder().application_id(crate::APP_ID).flags(flags).build();
    let state: Rc<RefCell<Option<Rc<App>>>> = Rc::new(RefCell::new(None));
    let first = Rc::new(std::cell::Cell::new(true));

    let startup_state = state.clone();
    let startup_options = options.clone();
    application.connect_startup(move |application| {
        gtk::Window::set_default_icon_name(crate::APP_ID);
        let manager = adw::StyleManager::default();
        if let Some(dark) = startup_options.snapshot_dark {
            manager.set_color_scheme(if dark { adw::ColorScheme::ForceDark } else { adw::ColorScheme::ForceLight });
        }
        if let Some(display) = gdk::Display::default() {
            install_style(&display);
        }
        let (sender, receiver) = async_channel::unbounded::<Message>();
        let app = App::new(application.clone(), startup_options.snapshot.is_some(), sender.clone());
        if let Some(display) = gdk::Display::default() {
            apply_keymap(&app, &display);
            if let Some(keyboard) = display.default_seat().and_then(|s| s.keyboard()) {
                let (weak, d) = (Rc::downgrade(&app), display.clone());
                keyboard.connect_changed(move |_| {
                    if let Some(app) = weak.upgrade() {
                        apply_keymap(&app, &d);
                    }
                });
            }
        }
        let quit = gio::SimpleAction::new("quit", None);
        let weak = Rc::downgrade(&app);
        quit.connect_activate(move |_, _| {
            if let Some(app) = weak.upgrade() {
                app.request_quit();
            }
        });
        application.add_action(&quit);
        if !app.snapshot {
            std::mem::forget(application.hold());
            if app.config.borrow().prefs.tray_icon {
                app.set_tray(tray::spawn(app.tray_state(), sender.clone(), tray_icons()));
            }
            watch_sleep(&app);
            watch_signals(sender.clone());
        }
        let loop_app = app.clone();
        glib::spawn_future_local(async move {
            while let Ok(message) = receiver.recv().await {
                loop_app.handle(message);
            }
        });
        *startup_state.borrow_mut() = Some(app);
    });

    let activate_state = state.clone();
    application.connect_activate(move |_| {
        let Some(app) = activate_state.borrow().clone() else { return };
        let is_first = first.replace(false);
        if is_first && options.background && !options.show_settings {
            return;
        }
        if is_first {
            apply_snapshot_options(&app, &options);
        }
        window::present(&app);
        if is_first {
            if let Some(path) = options.snapshot.clone() {
                if let Some((w, h)) = options.snapshot_size {
                    if let Some(window) = app.window.borrow().as_ref() {
                        window.set_default_size(w.max(980), h.max(700));
                    }
                }
                let capture = app.clone();
                glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                    if let Err(error) = capture_window(&capture, &path) {
                        eprintln!("Snapshot failed: {error}");
                    } else {
                        println!("UI snapshot: {path}");
                    }
                    capture.adw_app.quit();
                });
            }
        }
    });
    application.run_with_args(&[std::env::args().next().unwrap_or_else(|| "opennaga".into())])
}

fn apply_keymap(app: &Rc<App>, display: &gdk::Display) {
    let (layout, typing) = keymap::read(display);
    *app.layout.borrow_mut() = layout;
    app.set_typing(typing);
    app.notify();
}

fn apply_snapshot_options(app: &Rc<App>, options: &Options) {
    let mut ui = app.ui.borrow_mut();
    if let Some(button) = options.snapshot_button.filter(|b| (1..=19).contains(b)) {
        ui.selected = button;
        ui.top_view = button >= 13;
    }
    if options.snapshot_hypershift {
        ui.layer = MappingLayer::Hypershift;
    }
    if let Some(section) = options.snapshot_section.as_deref().and_then(Section::from_name) {
        ui.section = section;
    }
}

fn capture_window(app: &Rc<App>, path: &str) -> Result<(), String> {
    let window = app.window.borrow().clone().ok_or("no window")?;
    let content = window.content().ok_or("no content")?;
    let (width, height) = (content.width(), content.height());
    let paintable = gtk::WidgetPaintable::new(Some(&content));
    let snapshot = gtk::Snapshot::new();
    let background = if adw::StyleManager::default().is_dark() { gdk::RGBA::new(0.13, 0.13, 0.14, 1.0) } else { gdk::RGBA::new(0.98, 0.98, 0.98, 1.0) };
    snapshot.append_color(&background, &gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
    paintable.snapshot(&snapshot, width as f64, height as f64);
    let node = snapshot.to_node().ok_or("empty render")?;
    let renderer = window.renderer().ok_or("no renderer")?;
    let texture = renderer.render_texture(&node, None);
    texture.save_to_png(path).map_err(|e| e.to_string())
}
