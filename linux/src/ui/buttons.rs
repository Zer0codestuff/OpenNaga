//! The Buttons section: interactive mouse photo, assignment grid and the
//! action inspector.

use super::inspector::Inspector;
use super::state::App;
use super::widgets::*;
use crate::model::actions::{button_name, MappingLayer};
use gtk::gdk_pixbuf::{InterpType, Pixbuf};
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const PERSPECTIVE: &[u8] = include_bytes!("../../../Resources/Mouse/naga-perspective.png");
const TOP: &[u8] = include_bytes!("../../../Resources/Mouse/naga-top.png");

/// Side grid polygons in the 1024 x 1536 perspective photograph.
const SIDE: [(i32, [(f64, f64); 4]); 12] = [
    (1, [(122., 648.), (166., 626.), (183., 697.), (133., 716.)]),
    (2, [(168., 623.), (219., 599.), (240., 678.), (186., 698.)]),
    (3, [(222., 596.), (289., 567.), (319., 648.), (243., 678.)]),
    (4, [(129., 745.), (171., 723.), (184., 792.), (138., 811.)]),
    (5, [(175., 721.), (235., 697.), (249., 777.), (187., 794.)]),
    (6, [(238., 694.), (307., 671.), (339., 749.), (252., 776.)]),
    (7, [(132., 840.), (177., 818.), (186., 888.), (144., 908.)]),
    (8, [(180., 817.), (238., 793.), (254., 870.), (190., 891.)]),
    (9, [(241., 791.), (317., 767.), (348., 844.), (256., 872.)]),
    (10, [(141., 938.), (184., 918.), (193., 988.), (153., 1006.)]),
    (11, [(187., 914.), (242., 895.), (257., 969.), (195., 990.)]),
    (12, [(245., 891.), (319., 871.), (345., 943.), (259., 967.)]),
];

/// Top controls in the 1500 x 1000 top photograph, as fractions.
const TOP_CONTROLS: [(i32, f64, f64); 7] = [
    (13, 0.346, 0.125),
    (14, 0.350, 0.227),
    (18, 0.420, 0.320),
    (19, 0.595, 0.320),
    (15, 0.447, 0.240),
    (17, 0.501, 0.240),
    (16, 0.555, 0.240),
];

fn load(bytes: &'static [u8]) -> Option<Pixbuf> {
    let stream = gio::MemoryInputStream::from_bytes(&glib::Bytes::from_static(bytes));
    Pixbuf::from_stream(&stream, gio::Cancellable::NONE).ok()
}

fn point_in_polygon(x: f64, y: f64, points: &[(f64, f64)]) -> bool {
    let mut inside = false;
    let mut j = points.len() - 1;
    for i in 0..points.len() {
        let (xi, yi) = points[i];
        let (xj, yj) = points[j];
        if (yi > y) != (yj > y) && x < (xj - xi) * (y - yi) / (yj - yi) + xi {
            inside = !inside;
        }
        j = i;
    }
    inside
}

struct Diagram {
    area: gtk::DrawingArea,
    perspective: Option<Pixbuf>,
    top: Option<Pixbuf>,
    cache: RefCell<Option<(bool, i32, i32, Pixbuf)>>,
    hovered: Cell<Option<i32>>,
}

impl Diagram {
    fn image_rect(top: bool, width: f64, height: f64) -> (f64, f64, f64, f64) {
        let ratio = if top { 1.5 } else { 2.0 / 3.0 };
        let h = height.min(width / ratio);
        let w = h * ratio;
        ((width - w) / 2.0, (height - h) / 2.0, w, h)
    }

    fn hit(&self, top: bool, x: f64, y: f64) -> Option<i32> {
        let (ox, oy, w, h) = Self::image_rect(top, self.area.width() as f64, self.area.height() as f64);
        if w <= 0.0 || h <= 0.0 {
            return None;
        }
        if top {
            TOP_CONTROLS
                .iter()
                .find(|(_, fx, fy)| {
                    let (cx, cy) = (ox + fx * w, oy + fy * h);
                    (x - cx).powi(2) + (y - cy).powi(2) <= 13.0f64.powi(2)
                })
                .map(|(i, _, _)| *i)
        } else {
            let (u, v) = ((x - ox) / w * 1024.0, (y - oy) / h * 1536.0);
            SIDE.iter().find(|(_, points)| point_in_polygon(u, v, points)).map(|(i, _)| *i)
        }
    }

    fn scaled(&self, top: bool, w: i32, h: i32) -> Option<Pixbuf> {
        if let Some((cached_top, cw, ch, pixbuf)) = self.cache.borrow().as_ref() {
            if *cached_top == top && *cw == w && *ch == h {
                return Some(pixbuf.clone());
            }
        }
        let source = if top { self.top.as_ref() } else { self.perspective.as_ref() }?;
        let scaled = source.scale_simple(w.max(1), h.max(1), InterpType::Hyper)?;
        *self.cache.borrow_mut() = Some((top, w, h, scaled.clone()));
        Some(scaled)
    }

    fn draw(&self, app: &App, cr: &gtk::cairo::Context, width: i32, height: i32) {
        let (top, selected) = {
            let ui = app.ui.borrow();
            (ui.top_view, ui.selected)
        };
        let active = app.active_button.get();
        let scale = self.area.scale_factor() as f64;
        let (ox, oy, w, h) = Self::image_rect(top, width as f64, height as f64);
        if let Some(pixbuf) = self.scaled(top, (w * scale).round() as i32, (h * scale).round() as i32) {
            cr.save().ok();
            cr.translate(ox, oy);
            cr.scale(1.0 / scale, 1.0 / scale);
            cr.set_source_pixbuf(&pixbuf, 0.0, 0.0);
            cr.paint().ok();
            cr.restore().ok();
        }
        let green = (0.55, 1.0, 0.39);
        if top {
            for (index, fx, fy) in TOP_CONTROLS {
                let (cx, cy) = (ox + fx * w, oy + fy * h);
                cr.new_path();
                cr.arc(cx, cy, 11.5, 0.0, std::f64::consts::TAU);
                if selected == index {
                    cr.set_source_rgb(green.0, green.1, green.2);
                } else {
                    cr.set_source_rgba(0.0, 0.0, 0.0, 0.75);
                }
                cr.fill_preserve().ok();
                cr.set_source_rgba(1.0, 1.0, 1.0, 0.75);
                cr.set_line_width(if active == Some(index) { 2.5 } else { 1.0 });
                cr.stroke().ok();
                let text = match index {
                    15 => "←".to_string(),
                    16 => "→".to_string(),
                    _ => index.to_string(),
                };
                let layout = pangocairo::functions::create_layout(cr);
                layout.set_font_description(Some(&gtk::pango::FontDescription::from_string("Sans Semibold 7.5")));
                layout.set_text(&text);
                let (tw, th) = layout.pixel_size();
                if selected == index {
                    cr.set_source_rgb(0.0, 0.0, 0.0);
                } else {
                    cr.set_source_rgb(1.0, 1.0, 1.0);
                }
                cr.move_to(cx - tw as f64 / 2.0, cy - th as f64 / 2.0);
                pangocairo::functions::show_layout(cr, &layout);
                cr.new_path();
            }
        } else {
            for (index, points) in SIDE {
                let highlighted = selected == index || active == Some(index);
                let hovered = self.hovered.get() == Some(index);
                if !highlighted && !hovered {
                    continue;
                }
                for (i, (px, py)) in points.iter().enumerate() {
                    let (x, y) = (ox + px / 1024.0 * w, oy + py / 1536.0 * h);
                    if i == 0 {
                        cr.move_to(x, y);
                    } else {
                        cr.line_to(x, y);
                    }
                }
                cr.close_path();
                if highlighted {
                    cr.set_source_rgba(0.45, 0.92, 0.30, 0.35);
                    cr.fill_preserve().ok();
                    cr.set_source_rgb(green.0, green.1, green.2);
                } else {
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.65);
                }
                cr.set_line_width(if active == Some(index) { 2.5 } else { 1.5 });
                cr.stroke().ok();
            }
        }
    }
}

fn segmented(labels: &[&str]) -> (gtk::Box, Vec<gtk::ToggleButton>) {
    let container = hbox(0);
    container.add_css_class("linked");
    container.add_css_class("naga-segmented");
    let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
    for text in labels {
        let button = gtk::ToggleButton::with_label(text);
        button.set_hexpand(true);
        if let Some(first) = buttons.first() {
            button.set_group(Some(first));
        }
        container.append(&button);
        buttons.push(button);
    }
    (container, buttons)
}

struct Cell19 {
    index: i32,
    button: gtk::Button,
    summary: gtk::Label,
}

pub fn build(app: &Rc<App>) -> gtk::Widget {
    let page = hbox(0);
    let left = padded(16, 22);
    left.set_hexpand(true);

    let controls = hbox(12);
    controls.set_halign(gtk::Align::Center);
    let (view_box, view_buttons) = segmented(&["Side", "Top"]);
    view_box.set_size_request(160, -1);
    let (layer_box, layer_buttons) = segmented(&["Standard", "Hypershift"]);
    layer_box.set_size_request(200, -1);
    layer_box.set_tooltip_text(Some("Hypershift assignments apply while you hold the control assigned to Hypershift"));
    controls.append(&view_box);
    controls.append(&layer_box);
    left.append(&controls);

    let diagram = Rc::new(Diagram {
        area: gtk::DrawingArea::new(),
        perspective: load(PERSPECTIVE),
        top: load(TOP),
        cache: RefCell::new(None),
        hovered: Cell::new(None),
    });
    diagram.area.set_content_height(300);
    diagram.area.set_size_request(-1, 220);
    diagram.area.set_vexpand(true);
    let draw_app = app.clone();
    let draw_diagram = Rc::downgrade(&diagram);
    diagram.area.set_draw_func(move |_, cr, w, h| {
        if let Some(d) = draw_diagram.upgrade() {
            d.draw(&draw_app, cr, w, h);
        }
    });
    let click = gtk::GestureClick::new();
    let click_app = app.clone();
    let click_diagram = Rc::downgrade(&diagram);
    click.connect_released(move |_, _, x, y| {
        let Some(d) = click_diagram.upgrade() else { return };
        let top = click_app.ui.borrow().top_view;
        if let Some(index) = d.hit(top, x, y) {
            click_app.ui.borrow_mut().selected = index;
            click_app.notify();
        }
    });
    diagram.area.add_controller(click);
    let motion = gtk::EventControllerMotion::new();
    let motion_app = app.clone();
    let motion_diagram = Rc::downgrade(&diagram);
    motion.connect_motion(move |_, x, y| {
        let Some(d) = motion_diagram.upgrade() else { return };
        let top = motion_app.ui.borrow().top_view;
        let hit = d.hit(top, x, y);
        d.area.set_cursor_from_name(if hit.is_some() { Some("pointer") } else { None });
        let side_hover = if top { None } else { hit };
        if d.hovered.get() != side_hover {
            d.hovered.set(side_hover);
            d.area.queue_draw();
        }
        let tip = hit.map(button_name);
        if d.area.tooltip_text().map(|t| t.to_string()) != tip {
            d.area.set_tooltip_text(tip.as_deref());
        }
    });
    let leave_diagram = Rc::downgrade(&diagram);
    motion.connect_leave(move |_| {
        if let Some(d) = leave_diagram.upgrade() {
            d.hovered.set(None);
            d.area.queue_draw();
        }
    });
    diagram.area.add_controller(motion);
    left.append(&diagram.area);

    let heading = hbox(8);
    let grid_title = label("12-button side panel", &["heading", "naga-small"]);
    grid_title.set_hexpand(true);
    heading.append(&grid_title);
    heading.append(&label("Select on the photo or in the list", &["dim-label", "naga-tiny"]));
    left.append(&heading);

    let side_grid = gtk::Grid::new();
    let top_grid = gtk::Grid::new();
    for grid in [&side_grid, &top_grid] {
        grid.set_column_homogeneous(true);
        grid.set_row_spacing(6);
        grid.set_column_spacing(6);
    }
    let mut cells: Vec<Cell19> = Vec::new();
    for index in 1..=19 {
        let row = hbox(8);
        let badge = gtk::Label::new(Some(&index.to_string()));
        badge.add_css_class("naga-badge");
        badge.set_valign(gtk::Align::Center);
        row.append(&badge);
        let texts = vbox(2);
        texts.set_valign(gtk::Align::Center);
        if index >= 13 {
            let name = label(&button_name(index), &["naga-cell-name"]);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&name);
        }
        let summary = label("", &["naga-cell-summary"]);
        summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
        texts.append(&summary);
        row.append(&texts);
        let button = gtk::Button::new();
        button.set_child(Some(&row));
        button.add_css_class("flat");
        button.add_css_class("naga-cell");
        button.set_hexpand(true);
        let select_app = app.clone();
        button.connect_clicked(move |_| {
            {
                let mut ui = select_app.ui.borrow_mut();
                ui.selected = index;
                ui.top_view = index >= 13;
            }
            select_app.notify();
        });
        if index <= 12 {
            side_grid.attach(&button, (index - 1) % 3, (index - 1) / 3, 1, 1);
        } else {
            let position = index - 13;
            top_grid.attach(&button, position % 2, position / 2, 1, 1);
        }
        cells.push(Cell19 { index, button, summary });
    }
    left.append(&side_grid);
    left.append(&top_grid);

    let left_scroll = scrolled(&left);
    left_scroll.set_hexpand(true);
    left_scroll.set_size_request(390, -1);
    page.append(&left_scroll);
    page.append(&gtk::Separator::new(gtk::Orientation::Vertical));

    let inspector = Inspector::new(app);
    let inspector_scroll = scrolled(&inspector.root);
    inspector_scroll.set_size_request(340, -1);
    inspector_scroll.set_hexpand(false);
    inspector_scroll.add_css_class("naga-inspector");
    page.append(&inspector_scroll);

    let updating = Rc::new(Cell::new(false));
    for (i, button) in view_buttons.iter().enumerate() {
        let a = app.clone();
        let guard = updating.clone();
        button.connect_toggled(move |b| {
            if guard.get() || !b.is_active() {
                return;
            }
            a.ui.borrow_mut().top_view = i == 1;
            a.notify();
        });
    }
    for (i, button) in layer_buttons.iter().enumerate() {
        let a = app.clone();
        let guard = updating.clone();
        button.connect_toggled(move |b| {
            if guard.get() || !b.is_active() {
                return;
            }
            a.ui.borrow_mut().layer = if i == 0 { MappingLayer::Standard } else { MappingLayer::Hypershift };
            a.notify();
        });
    }

    let refresh_app = app.clone();
    let last_top = Cell::new(None::<bool>);
    app.listen(move || {
        let app = &refresh_app;
        let (top, layer, selected) = {
            let ui = app.ui.borrow();
            (ui.top_view, ui.layer, ui.selected)
        };
        updating.set(true);
        view_buttons[usize::from(top)].set_active(true);
        layer_buttons[if layer == MappingLayer::Standard { 0 } else { 1 }].set_active(true);
        updating.set(false);
        if last_top.get() != Some(top) {
            last_top.set(Some(top));
            diagram.hovered.set(None);
        }
        diagram.area.queue_draw();
        side_grid.set_visible(!top);
        top_grid.set_visible(top);
        grid_title.set_text(if top { "Top buttons" } else { "12-button side panel" });
        let standard = app.current_mapping(MappingLayer::Standard);
        let shifted = app.current_mapping(MappingLayer::Hypershift);
        let active = app.active_button.get();
        for cell in &cells {
            let summary = summary(&standard, &shifted, cell.index, layer);
            cell.summary.set_text(&summary);
            let secondary = cell.index >= 13 || (layer == MappingLayer::Hypershift && !shifted.contains_key(&cell.index));
            if secondary {
                cell.summary.add_css_class("dim-label");
            } else {
                cell.summary.remove_css_class("dim-label");
            }
            let tip = format!("{}: {}", button_name(cell.index), summary);
            if cell.button.tooltip_text().map(|t| t.to_string()) != Some(tip.clone()) {
                cell.button.set_tooltip_text(Some(&tip));
            }
            toggle_class(&cell.button, "selected", selected == cell.index);
            toggle_class(&cell.button, "active", active == Some(cell.index));
        }
        inspector.refresh();
    });
    page.upcast()
}

fn toggle_class(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    if on {
        widget.add_css_class(class);
    } else {
        widget.remove_css_class(class);
    }
}

/// The text the assignment list shows for a control in a layer.
pub fn summary(standard: &crate::model::profile::Mapping, shifted: &crate::model::profile::Mapping, index: i32, layer: MappingLayer) -> String {
    let standard_text = standard.get(&index).map(|a| a.display_name()).unwrap_or_else(|| "Original".into());
    if layer == MappingLayer::Standard {
        return standard_text;
    }
    if standard.get(&index).map(|a| a.is_hypershift()).unwrap_or(false) {
        return "Hypershift key".into();
    }
    shifted.get(&index).map(|a| a.display_name()).unwrap_or_else(|| format!("Standard: {standard_text}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hotspots() {
        let (_, square) = SIDE[0];
        assert!(point_in_polygon(150.0, 670.0, &square));
        assert!(!point_in_polygon(100.0, 670.0, &square));
        assert_eq!(Diagram::image_rect(false, 400.0, 300.0), (100.0, 0.0, 200.0, 300.0));
        assert_eq!(Diagram::image_rect(true, 300.0, 400.0), (0.0, 100.0, 300.0, 200.0));
    }
}
