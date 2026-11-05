//! Small widget helpers shared by the views.

use gtk::prelude::*;

pub fn label(text: &str, classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.set_xalign(0.0);
    for class in classes {
        label.add_css_class(class);
    }
    label
}

pub fn wrapped(text: &str, classes: &[&str]) -> gtk::Label {
    let label = label(text, classes);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_max_width_chars(70);
    label
}

pub fn dim(text: &str) -> gtk::Label {
    wrapped(text, &["dim-label"])
}

pub fn caption(text: &str) -> gtk::Label {
    wrapped(text, &["dim-label", "caption"])
}

pub fn hbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Horizontal, spacing)
}

pub fn vbox(spacing: i32) -> gtk::Box {
    gtk::Box::new(gtk::Orientation::Vertical, spacing)
}

pub fn spacer() -> gtk::Box {
    let b = hbox(0);
    b.set_hexpand(true);
    b
}

pub fn button(text: &str) -> gtk::Button {
    gtk::Button::with_label(text)
}

pub fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button
}

pub fn labelled_icon_button(icon: &str, text: &str) -> gtk::Button {
    let content = adw::ButtonContent::builder().icon_name(icon).label(text).build();
    let button = gtk::Button::new();
    button.set_child(Some(&content));
    button
}

pub fn dot(on: bool) -> gtk::Box {
    let dot = hbox(0);
    dot.add_css_class("naga-dot");
    dot.set_valign(gtk::Align::Center);
    if on {
        dot.add_css_class("on");
    }
    dot
}

pub fn set_dot(dot: &gtk::Box, on: bool) {
    if on {
        dot.add_css_class("on");
    } else {
        dot.remove_css_class("on");
    }
}

pub fn card(child: &impl IsA<gtk::Widget>) -> gtk::Box {
    let card = vbox(0);
    card.add_css_class("card");
    card.append(child);
    card
}

pub fn group(spacing: i32) -> (gtk::Box, gtk::Box) {
    let inner = vbox(spacing);
    inner.add_css_class("naga-group");
    (card(&inner), inner)
}

pub fn warning(text: &str) -> gtk::Box {
    let row = hbox(8);
    let icon = gtk::Image::from_icon_name("opennaga-warning-symbolic");
    icon.set_valign(gtk::Align::Start);
    icon.add_css_class("naga-warning");
    let text = wrapped(text, &["naga-warning"]);
    text.set_hexpand(true);
    row.append(&icon);
    row.append(&text);
    row
}

pub fn info(icon: &str, text: &str) -> gtk::Box {
    let row = hbox(8);
    let image = gtk::Image::from_icon_name(icon);
    image.set_valign(gtk::Align::Start);
    let text = wrapped(text, &[]);
    text.set_hexpand(true);
    row.append(&image);
    row.append(&text);
    row
}

pub fn clear(container: &gtk::Box) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_child(Some(child));
    scroll.set_vexpand(true);
    scroll
}

pub fn padded(spacing: i32, margin: i32) -> gtk::Box {
    let b = vbox(spacing);
    b.set_margin_top(margin);
    b.set_margin_bottom(margin);
    b.set_margin_start(margin);
    b.set_margin_end(margin);
    b
}

/// A drop-down over plain strings, with a quiet setter for refreshes.
pub fn string_dropdown(items: &[String]) -> gtk::DropDown {
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    gtk::DropDown::from_strings(&refs)
}

pub fn dropdown_items(dropdown: &gtk::DropDown) -> Vec<String> {
    let Some(model) = dropdown.model().and_then(|m| m.downcast::<gtk::StringList>().ok()) else { return vec![] };
    (0..model.n_items()).filter_map(|i| model.string(i).map(|s| s.to_string())).collect()
}

pub fn set_dropdown_items(dropdown: &gtk::DropDown, items: &[String]) {
    if dropdown_items(dropdown) == items {
        return;
    }
    let refs: Vec<&str> = items.iter().map(String::as_str).collect();
    dropdown.set_model(Some(&gtk::StringList::new(&refs)));
}

pub fn dropdown_text(dropdown: &gtk::DropDown) -> Option<String> {
    dropdown.selected_item().and_then(|o| o.downcast::<gtk::StringObject>().ok()).map(|s| s.string().to_string())
}

pub fn text_view(text: &str, height: i32) -> (gtk::ScrolledWindow, gtk::TextView) {
    let view = gtk::TextView::new();
    view.buffer().set_text(text);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.add_css_class("naga-textview");
    view.set_top_margin(6);
    view.set_bottom_margin(6);
    view.set_left_margin(6);
    view.set_right_margin(6);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&view));
    scroll.set_min_content_height(height);
    scroll.add_css_class("card");
    (scroll, view)
}

pub fn text_of(view: &gtk::TextView) -> String {
    let buffer = view.buffer();
    buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).to_string()
}
