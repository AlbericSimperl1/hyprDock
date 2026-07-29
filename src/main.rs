use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box, Button, CssProvider, DrawingArea, EventControllerMotion,
    GestureClick, IconTheme, Orientation, Overlay, Popover, Separator,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, PI};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct DockApp {
    name: String,
    cmd: String,
    icon: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct HyprClient {
    address: String,
    class: String,
    title: String,
}

fn get_config_path() -> PathBuf {
    let mut path = glib::user_config_dir();
    path.push("hyprDock");
    fs::create_dir_all(&path).ok();
    path.push("pins.json");
    path
}

fn get_css_path() -> PathBuf {
    let mut path = glib::user_config_dir();
    path.push("hyprDock");
    fs::create_dir_all(&path).ok();
    path.push("style.css");
    path
}

fn load_pins() -> Vec<DockApp> {
    let path = get_config_path();
    if path.exists() {
        if let Ok(data) = fs::read_to_string(&path) {
            if let Ok(apps) = serde_json::from_str(&data) {
                return apps;
            }
        }
    }

    let default_apps = vec![
        DockApp {
            name: "Terminal".into(),
            cmd: "kitty".into(),
            icon: "utilities-terminal".into(),
        },
        DockApp {
            name: "Browser".into(),
            cmd: "firefox".into(),
            icon: "firefox".into(),
        },
        DockApp {
            name: "Files".into(),
            cmd: "thunar".into(),
            icon: "system-file-manager".into(),
        },
        DockApp {
            name: "Code".into(),
            cmd: "code".into(),
            icon: "com.visualstudio.code".into(),
        },
    ];

    save_pins(&default_apps);
    default_apps
}

fn save_pins(apps: &[DockApp]) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(apps) {
        let _ = fs::write(path, json);
    }
}

fn get_running_clients() -> Vec<HyprClient> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .output()
        .ok();

    if let Some(out) = output {
        if out.status.success() {
            if let Ok(clients) = serde_json::from_slice::<Vec<HyprClient>>(&out.stdout) {
                return clients;
            }
        }
    }
    vec![]
}

fn get_active_workspace_windows() -> Option<i64> {
    let output = Command::new("hyprctl")
        .args(["activeworkspace", "-j"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let json: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    json.get("windows")?.as_i64()
}

fn main() {
    let app = Application::builder()
        .application_id("com.omarchy.hyprdock")
        .build();

    app.connect_activate(build_ui);
    app.run();
}

fn build_ui(app: &Application) {
    let window = ApplicationWindow::builder()
        .application(app)
        .title("hyprDock")
        .build();

    window.init_layer_shell();
    window.set_namespace("hyprdock");
    window.set_layer(Layer::Top);
    window.set_keyboard_mode(KeyboardMode::None);
    window.set_anchor(Edge::Bottom, true);
    window.set_margin(Edge::Bottom, 0);

    apply_css();

    // let pinned_apps = Rc::new(RefCell::new(load_pins()));
    // let container = Box::new(Orientation::Horizontal, 8);
    // container.add_css_class("dock-container");

    // let corner_left = Box::new(Orientation::Horizontal, 0);
    // corner_left.add_css_class("dock-corner-left");
    // corner_left.set_valign(gtk4::Align::End);
    // corner_left.set_size_request(16, 16);

    // let corner_right = Box::new(Orientation::Horizontal, 0);
    // corner_right.add_css_class("dock-corner-right");
    // corner_right.set_valign(gtk4::Align::End);
    // corner_right.set_size_request(16, 16);

    // let wrapper = Box::new(Orientation::Horizontal, 0);
    // wrapper.set_valign(gtk4::Align::End);
    // wrapper.append(&corner_left);
    // wrapper.append(&container);
    // wrapper.append(&corner_right);

    // let is_hovered = Rc::new(RefCell::new(false));
    // let last_state = Rc::new(RefCell::new(String::new()));

    // render_dock_items(&container, &pinned_apps);
    // window.set_child(Some(&wrapper));

    // in build_ui: vervang container/wrapper opbouw door dit
    let pinned_apps = Rc::new(RefCell::new(load_pins()));
    let container = Box::new(Orientation::Horizontal, 8);
    container.add_css_class("dock-container");

    let shape = DrawingArea::new();
    shape.set_can_target(false); // klikken moeten door naar de knoppen
    shape.set_hexpand(true);
    shape.set_vexpand(true);
    shape.set_draw_func(move |_, cr, w, h| {
        draw_dock_shape(cr, w as f64, h as f64);
    });

    let overlay = Overlay::new();
    overlay.set_child(Some(&shape)); // bepaalt de grootte
    overlay.add_overlay(&container); // tekent de achtergrondvorm erachter/eronder

    let is_hovered = Rc::new(RefCell::new(false));
    let last_state = Rc::new(RefCell::new(String::new()));

    render_dock_items(&container, &pinned_apps);
    window.set_child(Some(&overlay));

    let motion_controller = EventControllerMotion::new();
    let is_hovered_enter = is_hovered.clone();
    let win_enter = window.clone();

    motion_controller.connect_enter(move |_, _, _| {
        *is_hovered_enter.borrow_mut() = true;
        win_enter.set_margin(Edge::Bottom, 0);
    });

    let is_hovered_leave = is_hovered.clone();
    let win_leave = window.clone();

    motion_controller.connect_leave(move |_| {
        *is_hovered_leave.borrow_mut() = false;
        check_and_update_autohide(&win_leave, false);
    });

    window.add_controller(motion_controller);

    let win_poll = window.clone();
    let is_hovered_poll = is_hovered.clone();
    let container_poll = container.clone();
    let pinned_poll = pinned_apps.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(300), move || {
        let hovered = *is_hovered_poll.borrow();
        let current_pins = pinned_poll.borrow().clone();
        let current_clients = get_running_clients();

        let fingerprint = format!(
            "{:?}-{:?}",
            current_pins,
            current_clients
                .iter()
                .map(|c| (&c.address, &c.class))
                .collect::<Vec<_>>()
        );

        let mut prev_state = last_state.borrow_mut();
        if *prev_state != fingerprint {
            *prev_state = fingerprint;
            render_dock_items(&container_poll, &pinned_poll);
        }

        check_and_update_autohide(&win_poll, hovered);

        glib::ControlFlow::Continue
    });

    window.present();
}

fn check_and_update_autohide(window: &ApplicationWindow, is_hovered: bool) {
    if is_hovered {
        window.set_margin(Edge::Bottom, 0);
        return;
    }

    match get_active_workspace_windows() {
        Some(windows) if windows > 0 => {
            window.set_margin(Edge::Bottom, -56);
        }
        _ => {
            window.set_margin(Edge::Bottom, 0);
        }
    }
}

fn render_dock_items(container: &Box, pinned_apps: &Rc<RefCell<Vec<DockApp>>>) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    let pins = pinned_apps.borrow().clone();
    let clients = get_running_clients();

    for (index, app_info) in pins.iter().enumerate() {
        let matching_client = clients.iter().find(|c| {
            let class_lower = c.class.to_lowercase();
            let app_cmd = app_info.cmd.to_lowercase();
            let app_name = app_info.name.to_lowercase();
            class_lower.contains(&app_cmd)
                || app_cmd.contains(&class_lower)
                || class_lower.contains(&app_name)
        });

        let is_running = matching_client.is_some();
        let btn = create_dock_button(&app_info.icon, &app_info.name, is_running);

        let cmd = app_info.cmd.clone();
        let client_address = matching_client.map(|c| c.address.clone());

        btn.connect_clicked(move |_| {
            if let Some(ref addr) = client_address {
                let _ = Command::new("hyprctl")
                    .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
                    .spawn();
            } else {
                let _ = Command::new("sh").arg("-c").arg(&cmd).spawn();
            }
        });

        let gesture = GestureClick::new();
        gesture.set_button(3);
        let pinned_apps_clone = pinned_apps.clone();
        let container_clone = container.clone();
        let btn_clone = btn.clone();

        gesture.connect_pressed(move |_, _, _, _| {
            let popover = Popover::new();
            let unpin_btn = Button::with_label("Ontpinnen van hyprDock");
            unpin_btn.add_css_class("popover-btn");

            let pinned_apps_inner = pinned_apps_clone.clone();
            let container_inner = container_clone.clone();
            let popover_clone = popover.clone();

            unpin_btn.connect_clicked(move |_| {
                pinned_apps_inner.borrow_mut().remove(index);
                save_pins(&pinned_apps_inner.borrow());
                render_dock_items(&container_inner, &pinned_apps_inner);
                popover_clone.popdown();
            });

            popover.set_child(Some(&unpin_btn));
            popover.set_parent(&btn_clone);
            popover.popup();
        });

        btn.add_controller(gesture);
        container.append(&btn);
    }

    let unpinned_clients: Vec<&HyprClient> = clients
        .iter()
        .filter(|client| {
            !pins.iter().any(|pin| {
                let class_lower = client.class.to_lowercase();
                let pin_cmd = pin.cmd.to_lowercase();
                let pin_name = pin.name.to_lowercase();
                class_lower.contains(&pin_cmd)
                    || pin_cmd.contains(&class_lower)
                    || class_lower.contains(&pin_name)
            })
        })
        .collect();

    if !unpinned_clients.is_empty() {
        let sep = Separator::new(Orientation::Vertical);
        sep.add_css_class("dock-separator");
        container.append(&sep);

        for client in unpinned_clients {
            let icon_name = client.class.to_lowercase();
            let btn = create_dock_button(&icon_name, &client.title, true);

            let addr = client.address.clone();
            btn.connect_clicked(move |_| {
                let _ = Command::new("hyprctl")
                    .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
                    .spawn();
            });

            let gesture = GestureClick::new();
            gesture.set_button(3);
            let pinned_apps_clone = pinned_apps.clone();
            let container_clone = container.clone();
            let btn_clone = btn.clone();
            let client_class = client.class.clone();
            let client_title = client.title.clone();

            gesture.connect_pressed(move |_, _, _, _| {
                let popover = Popover::new();
                let pin_btn = Button::with_label("Vastpinnen aan hyprDock");
                pin_btn.add_css_class("popover-btn");

                let pinned_apps_inner = pinned_apps_clone.clone();
                let container_inner = container_clone.clone();
                let popover_clone = popover.clone();
                let c_class = client_class.clone();
                let c_title = client_title.clone();

                pin_btn.connect_clicked(move |_| {
                    let new_app = DockApp {
                        name: c_title.clone(),
                        cmd: c_class.to_lowercase(),
                        icon: c_class.to_lowercase(),
                    };
                    pinned_apps_inner.borrow_mut().push(new_app);
                    save_pins(&pinned_apps_inner.borrow());
                    render_dock_items(&container_inner, &pinned_apps_inner);
                    popover_clone.popdown();
                });

                popover.set_child(Some(&pin_btn));
                popover.set_parent(&btn_clone);
                popover.popup();
            });

            btn.add_controller(gesture);
            container.append(&btn);
        }
    }
}

fn create_dock_button(icon_name: &str, tooltip: &str, is_running: bool) -> Button {
    let btn = Button::builder().build();
    btn.add_css_class("dock-button");

    let item_box = Box::new(Orientation::Vertical, 2);

    let display = gtk4::gdk::Display::default().expect("Geen GDK display gevonden");
    let icon_theme = IconTheme::for_display(&display);

    let clean_name = icon_name.to_lowercase();
    let valid_icon = if icon_theme.has_icon(icon_name) {
        icon_name.to_string()
    } else if icon_theme.has_icon(&clean_name) {
        clean_name
    } else if clean_name.contains("zen") && icon_theme.has_icon("zen-browser") {
        "zen-browser".to_string()
    } else {
        "application-x-executable".to_string()
    };

    let image = gtk4::Image::from_icon_name(&valid_icon);
    image.set_pixel_size(35);
    item_box.append(&image);

    if is_running {
        let dot = Box::new(Orientation::Horizontal, 0);
        // Verander de class naam
        dot.add_css_class("running-pill");

        // Centreer de pill onder het icoon
        dot.set_halign(gtk4::Align::Center);

        // Forceer de pill afmetingen (bijv. 20px breed, 5px hoog)
        dot.set_size_request(20, 5);

        item_box.append(&dot);
    }

    btn.set_child(Some(&item_box));
    btn.set_tooltip_text(Some(tooltip));
    btn
}

use std::time::SystemTime;

fn apply_css() {
    let provider = CssProvider::new();
    let css_path = get_css_path();

    // Maak standaard CSS aan als het bestand nog niet bestaat
    if !css_path.exists() {
        let default_css = "..."; // Je standaard CSS string
        let _ = fs::write(&css_path, default_css);
    }

    // Eerste keer laden
    provider.load_from_path(&css_path);

    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    // --- HOT RELOAD LOGICA ---
    // Onthoud wanneer het bestand voor het laatst is bewerkt
    let mut last_modified: Option<SystemTime> =
        fs::metadata(&css_path).and_then(|m| m.modified()).ok();

    let provider_clone = provider.clone();
    let css_path_clone = css_path.clone();

    // Check elke seconde of het bestand is gewijzigd
    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        if let Ok(metadata) = fs::metadata(&css_path_clone) {
            if let Ok(modified) = metadata.modified() {
                if last_modified != Some(modified) {
                    last_modified = Some(modified);

                    // GTK4 ververst alle stijlen direct op het scherm!
                    provider_clone.load_from_path(&css_path_clone);
                    println!("css reloaded");
                }
            }
        }
        glib::ControlFlow::Continue
    });
}

// nieuwe functie, ergens toevoegen (bv. onder create_dock_button)
fn draw_dock_shape(cr: &gtk4::cairo::Context, w: f64, h: f64) {
    // --- tweak deze constanten ---
    let top_r: f64 = 14.0; // radius bovenste hoeken
    let flare_span: f64 = 40.0; // hoogte waarover de zijkant begint uit te vloeien
    let reach: f64 = 20.0; // hoe ver de haak naar buiten uitsteekt
    let hook: f64 = 14.0; // hoogte van de kleine terugkrul onderaan
    let bg = (0.117, 0.117, 0.180, 0.933); // r,g,b,a achtergrond (#1e1e2eee)
    let border = (1.0, 1.0, 1.0, 0.08); // r,g,b,a rand
                                        // -----------------------------

    let flare_y0 = h - flare_span;

    cr.new_path();
    cr.move_to(top_r, 0.0);
    cr.line_to(w - top_r, 0.0);
    cr.arc(w - top_r, top_r, top_r, -FRAC_PI_2, 0.0);
    cr.line_to(w, flare_y0);
    cr.curve_to(
        w,
        flare_y0 + flare_span * 0.5,
        w + reach,
        h - hook - flare_span * 0.15,
        w + reach,
        h - hook,
    );
    cr.curve_to(w + reach, h - hook * 0.3, w + reach * 0.4, h, w, h);
    cr.line_to(0.0, h);
    cr.curve_to(-reach * 0.4, h, -reach, h - hook * 0.3, -reach, h - hook);
    cr.curve_to(
        -reach,
        h - hook - flare_span * 0.15,
        0.0,
        flare_y0 + flare_span * 0.5,
        0.0,
        flare_y0,
    );
    cr.line_to(0.0, top_r);
    cr.arc(top_r, top_r, top_r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();

    cr.set_source_rgba(bg.0, bg.1, bg.2, bg.3);
    let _ = cr.fill_preserve();
    cr.set_source_rgba(border.0, border.1, border.2, border.3);
    cr.set_line_width(1.2);
    let _ = cr.stroke();
}
