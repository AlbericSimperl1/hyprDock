// mod animations;
use gtk4::prelude::*;
use gtk4::{
    Application, ApplicationWindow, Box, Button, CssProvider, EventControllerMotion, GestureClick,
    IconTheme, Orientation, Overlay, Popover, Separator,
};
use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::f64::consts::{FRAC_PI_2, PI};
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::rc::Rc;
use std::time::SystemTime;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
struct DockApp {
    name: String,
    cmd: String,
    icon: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct HyprWorkspaceInfo {
    id: i64,
    name: String,
}

#[derive(Deserialize, Debug, Clone, PartialEq)]
struct HyprClient {
    address: String,
    class: String,
    title: String,
    #[serde(default)]
    workspace: Option<HyprWorkspaceInfo>,
}

fn truncate_text(text: &str, max_len: usize) -> String {
    if text.chars().count() > max_len {
        let truncated: String = text.chars().take(max_len - 3).collect();
        format!("{}...", truncated)
    } else {
        text.to_string()
    }
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

    let pinned_apps = Rc::new(RefCell::new(load_pins()));
    let container = Box::new(Orientation::Horizontal, 8);
    container.add_css_class("dock-container");

    let bg = gtk4::DrawingArea::new();
    bg.set_draw_func(|_area, cr, width, height| {
        draw_dock_shape(cr, width as f64, height as f64);
    });

    let overlay = Overlay::new();
    overlay.set_child(Some(&bg));
    overlay.add_overlay(&container);
    overlay.set_measure_overlay(&container, true);

    let is_hovered = Rc::new(RefCell::new(false));
    let last_state = Rc::new(RefCell::new(String::new()));

    render_dock_items(&container, &pinned_apps);
    bg.queue_draw();
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
    let bg_poll = bg.clone();

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
            bg_poll.queue_draw();
        }

        check_and_update_autohide(&win_poll, hovered);

        glib::ControlFlow::Continue
    });

    window.present();
}

fn draw_dock_shape(cr: &gtk4::cairo::Context, width: f64, height: f64) {
    let nd: f64 = 10.0_f64.min(height * 0.5);
    let corner: f64 = 20.0_f64.min(height - nd);

    let quad = |cr: &gtk4::cairo::Context, sx: f64, sy: f64, cx: f64, cy: f64, ex: f64, ey: f64| {
        let cp1x = sx + 2.0 / 3.0 * (cx - sx);
        let cp1y = sy + 2.0 / 3.0 * (cy - sy);
        let cp2x = ex + 2.0 / 3.0 * (cx - ex);
        let cp2y = ey + 2.0 / 3.0 * (cy - ey);
        cr.curve_to(cp1x, cp1y, cp2x, cp2y, ex, ey);
    };

    let construct_path = || {
        cr.new_path();
        cr.move_to(width, height);

        quad(
            cr,
            width,
            height,
            width - nd,
            height,
            width - nd,
            height - nd,
        );

        cr.line_to(width - nd, corner);

        cr.arc_negative(width - nd - corner, corner, corner, 0.0, -FRAC_PI_2);

        cr.line_to(nd + corner, 0.0);

        cr.arc_negative(nd + corner, corner, corner, -FRAC_PI_2, -PI);

        cr.line_to(nd, height - nd);

        quad(cr, nd, height - nd, nd, height, 0.0, height);
    };

    construct_path();
    cr.close_path();
    cr.set_source_rgba(17.0 / 255.0, 17.0 / 255.0, 23.0 / 255.0, 0.55);
    let _ = cr.fill();

    construct_path();
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.17);
    cr.set_line_width(3.0);
    let _ = cr.stroke();
}

fn check_and_update_autohide(window: &ApplicationWindow, is_hovered: bool) {
    if is_hovered {
        window.set_margin(Edge::Bottom, 0);
        return;
    }

    match get_active_workspace_windows() {
        Some(windows) if windows > 0 => {
            window.set_margin(Edge::Bottom, -55);
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
        let matching_clients: Vec<HyprClient> = clients
            .iter()
            .filter(|c| {
                let class_lower = c.class.to_lowercase();
                let app_cmd = app_info.cmd.to_lowercase();
                let app_name = app_info.name.to_lowercase();
                class_lower.contains(&app_cmd)
                    || app_cmd.contains(&class_lower)
                    || class_lower.contains(&app_name)
            })
            .cloned()
            .collect();

        let is_running = !matching_clients.is_empty();
        let btn = create_dock_button(&app_info.icon, &app_info.name, is_running);

        let cmd = app_info.cmd.clone();
        let first_client_addr = matching_clients.first().map(|c| c.address.clone());

        // Linker muisklik
        let addr_click = first_client_addr.clone();
        let cmd_click = cmd.clone();
        btn.connect_clicked(move |_| {
            if let Some(ref addr) = addr_click {
                let _ = Command::new("hyprctl")
                    .args(["dispatch", "focuswindow", &format!("address:{}", addr)])
                    .spawn();
            } else {
                let _ = Command::new("sh").arg("-c").arg(&cmd_click).spawn();
            }
        });

        // Middle-click: Nieuw venster
        let middle_gesture = GestureClick::new();
        middle_gesture.set_button(2);
        let cmd_middle = cmd.clone();
        middle_gesture.connect_pressed(move |_, _, _, _| {
            let _ = Command::new("sh").arg("-c").arg(&cmd_middle).spawn();
        });
        btn.add_controller(middle_gesture);

        // Rechter muisklik: Menu
        let gesture = GestureClick::new();
        gesture.set_button(3);
        let pinned_apps_clone = pinned_apps.clone();
        let container_clone = container.clone();
        let btn_clone = btn.clone();
        let app_cmd_menu = app_info.cmd.clone();
        let app_name_menu = app_info.name.clone();

        gesture.connect_pressed(move |_, _, _, _| {
            let popover = Popover::new();
            popover.set_has_arrow(false);

            let popover_box = Box::new(Orientation::Vertical, 2);
            popover_box.add_css_class("popover-box");

            // 1. Naam van de App
            let header_label = gtk4::Label::new(Some(&app_name_menu));
            header_label.add_css_class("menu-header");
            header_label.set_xalign(0.0);
            popover_box.append(&header_label);

            let sep1 = Separator::new(Orientation::Horizontal);
            sep1.add_css_class("popover-separator");
            popover_box.append(&sep1);

            // 2. Unpin optie
            let unpin_box = Box::new(Orientation::Horizontal, 0);
            unpin_box.add_css_class("menu-item-row");
            let unpin_label = gtk4::Label::new(Some("Unpin from dock"));
            unpin_label.set_xalign(0.0);
            unpin_label.set_hexpand(true);
            unpin_box.append(&unpin_label);

            let click_unpin = GestureClick::new();
            let pinned_apps_inner = pinned_apps_clone.clone();
            let container_inner = container_clone.clone();
            let popover_unpin = popover.clone();
            click_unpin.connect_pressed(move |_, _, _, _| {
                pinned_apps_inner.borrow_mut().remove(index);
                save_pins(&pinned_apps_inner.borrow());
                render_dock_items(&container_inner, &pinned_apps_inner);
                popover_unpin.popdown();
            });
            unpin_box.add_controller(click_unpin);
            popover_box.append(&unpin_box);

            // 3. Lijst van actieve vensters
            if !matching_clients.is_empty() {
                let sep2 = Separator::new(Orientation::Horizontal);
                sep2.add_css_class("popover-separator");
                popover_box.append(&sep2);

                for client in &matching_clients {
                    let item_box = Box::new(Orientation::Horizontal, 8);
                    item_box.add_css_class("menu-item-row");

                    let ws_str = client
                        .workspace
                        .as_ref()
                        .map(|w| format!("w{}", w.id))
                        .unwrap_or_else(|| "w?".to_string());

                    let title_text = if client.title.is_empty() {
                        "Venster".to_string()
                    } else {
                        client.title.clone()
                    };

                    let full_text = format!("{}: {}", ws_str, title_text);
                    let truncated = truncate_text(&full_text, 25);

                    let win_label = gtk4::Label::new(Some(&truncated));
                    win_label.set_xalign(0.0);
                    win_label.set_hexpand(true);
                    win_label.add_css_class("win-label");

                    let addr_focus = client.address.clone();
                    let popover_focus = popover.clone();
                    let click_win = GestureClick::new();
                    click_win.connect_pressed(move |_, _, _, _| {
                        let _ = Command::new("hyprctl")
                            .args([
                                "dispatch",
                                "focuswindow",
                                &format!("address:{}", addr_focus),
                            ])
                            .spawn();
                        popover_focus.popdown();
                    });
                    item_box.add_controller(click_win);
                    item_box.append(&win_label);

                    // Kruisje (✕)
                    let close_label = gtk4::Label::new(Some("✕"));
                    close_label.add_css_class("close-btn-label");
                    let addr_close = client.address.clone();
                    let popover_close = popover.clone();

                    let click_close = GestureClick::new();
                    click_close.connect_pressed(move |_, _, _, _| {
                        let _ = Command::new("hyprctl")
                            .args([
                                "dispatch",
                                "closewindow",
                                &format!("address:{}", addr_close),
                            ])
                            .spawn();
                        popover_close.popdown();
                    });
                    close_label.add_controller(click_close);
                    item_box.append(&close_label);

                    popover_box.append(&item_box);
                }

                // 4. Plusknop (+)
                let add_box = Box::new(Orientation::Horizontal, 0);
                add_box.add_css_class("menu-item-row");
                let add_label = gtk4::Label::new(Some("+"));
                add_label.set_xalign(0.0);
                add_label.add_css_class("add-label");
                add_box.append(&add_label);

                let cmd_add = app_cmd_menu.clone();
                let popover_add = popover.clone();
                let click_add = GestureClick::new();
                click_add.connect_pressed(move |_, _, _, _| {
                    let _ = Command::new("sh").arg("-c").arg(&cmd_add).spawn();
                    popover_add.popdown();
                });
                add_box.add_controller(click_add);
                popover_box.append(&add_box);
            }

            popover.set_child(Some(&popover_box));
            popover.set_parent(&btn_clone);
            popover.popup();
        });

        btn.add_controller(gesture);
        container.append(&btn);
    }

    // Ongepinde geopende vensters
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

            let middle_gesture = GestureClick::new();
            middle_gesture.set_button(2);
            let client_class_mid = client.class.clone();
            middle_gesture.connect_pressed(move |_, _, _, _| {
                let _ = Command::new("sh")
                    .arg("-c")
                    .arg(client_class_mid.to_lowercase())
                    .spawn();
            });
            btn.add_controller(middle_gesture);

            let gesture = GestureClick::new();
            gesture.set_button(3);
            let pinned_apps_clone = pinned_apps.clone();
            let container_clone = container.clone();
            let btn_clone = btn.clone();
            let client_class = client.class.clone();
            let client_title = client.title.clone();
            let client_address = client.address.clone();
            let client_ws_str = client
                .workspace
                .as_ref()
                .map(|w| format!("w{}", w.id))
                .unwrap_or_else(|| "w?".to_string());

            gesture.connect_pressed(move |_, _, _, _| {
                let popover = Popover::new();
                popover.set_has_arrow(false);

                let popover_box = Box::new(Orientation::Vertical, 2);
                popover_box.add_css_class("popover-box");

                let header_label = gtk4::Label::new(Some(&client_class));
                header_label.add_css_class("menu-header");
                header_label.set_xalign(0.0);
                popover_box.append(&header_label);

                let sep1 = Separator::new(Orientation::Horizontal);
                sep1.add_css_class("popover-separator");
                popover_box.append(&sep1);

                let pin_box = Box::new(Orientation::Horizontal, 0);
                pin_box.add_css_class("menu-item-row");
                let pin_label = gtk4::Label::new(Some("Pin to dock"));
                pin_label.set_xalign(0.0);
                pin_label.set_hexpand(true);
                pin_box.append(&pin_label);

                let pinned_apps_inner = pinned_apps_clone.clone();
                let container_inner = container_clone.clone();
                let popover_pin = popover.clone();
                let c_class = client_class.clone();
                let c_title = client_title.clone();

                let click_pin = GestureClick::new();
                click_pin.connect_pressed(move |_, _, _, _| {
                    let new_app = DockApp {
                        name: c_title.clone(),
                        cmd: c_class.to_lowercase(),
                        icon: c_class.to_lowercase(),
                    };
                    pinned_apps_inner.borrow_mut().push(new_app);
                    save_pins(&pinned_apps_inner.borrow());
                    render_dock_items(&container_inner, &pinned_apps_inner);
                    popover_pin.popdown();
                });
                pin_box.add_controller(click_pin);
                popover_box.append(&pin_box);

                let sep2 = Separator::new(Orientation::Horizontal);
                sep2.add_css_class("popover-separator");
                popover_box.append(&sep2);

                let item_box = Box::new(Orientation::Horizontal, 8);
                item_box.add_css_class("menu-item-row");

                let full_text = format!("{}: {}", client_ws_str, client_title);
                let truncated = truncate_text(&full_text, 25);

                let win_label = gtk4::Label::new(Some(&truncated));
                win_label.set_xalign(0.0);
                win_label.set_hexpand(true);
                win_label.add_css_class("win-label");

                let addr_focus = client_address.clone();
                let popover_focus = popover.clone();
                let click_win = GestureClick::new();
                click_win.connect_pressed(move |_, _, _, _| {
                    let _ = Command::new("hyprctl")
                        .args([
                            "dispatch",
                            "focuswindow",
                            &format!("address:{}", addr_focus),
                        ])
                        .spawn();
                    popover_focus.popdown();
                });
                item_box.add_controller(click_win);
                item_box.append(&win_label);

                let close_label = gtk4::Label::new(Some("✕"));
                close_label.add_css_class("close-btn-label");
                let addr_close = client_address.clone();
                let popover_close = popover.clone();

                let click_close = GestureClick::new();
                click_close.connect_pressed(move |_, _, _, _| {
                    let _ = Command::new("hyprctl")
                        .args([
                            "dispatch",
                            "closewindow",
                            &format!("address:{}", addr_close),
                        ])
                        .spawn();
                    popover_close.popdown();
                });
                close_label.add_controller(click_close);
                item_box.append(&close_label);

                popover_box.append(&item_box);

                let add_box = Box::new(Orientation::Horizontal, 0);
                add_box.add_css_class("menu-item-row");
                let add_label = gtk4::Label::new(Some("+"));
                add_label.set_xalign(0.0);
                add_label.add_css_class("add-label");
                add_box.append(&add_label);

                let cmd_add = client_class.to_lowercase();
                let popover_add = popover.clone();
                let click_add = GestureClick::new();
                click_add.connect_pressed(move |_, _, _, _| {
                    let _ = Command::new("sh").arg("-c").arg(&cmd_add).spawn();
                    popover_add.popdown();
                });
                add_box.add_controller(click_add);
                popover_box.append(&add_box);

                popover.set_child(Some(&popover_box));
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
    image.set_pixel_size(32);
    item_box.append(&image);

    if is_running {
        let dot = Box::new(Orientation::Horizontal, 0);
        dot.add_css_class("running-dot");
        item_box.append(&dot);
    }

    btn.set_child(Some(&item_box));
    btn.set_tooltip_text(Some(tooltip));
    btn
}

fn apply_css() {
    let provider = CssProvider::new();
    let css_path = get_css_path();

    let default_css = r#"
window {
    background-color: transparent;
}

.dock-container {
    background-color: transparent;
    padding: 6px 16px;
    margin: 6px;
}

.dock-button {
    background-color: transparent;
    border: none;
    border-radius: 14px;
    padding: 4px;
    min-width: 44px;
    min-height: 44px;
    transition: background-color 150ms ease;
}

.dock-button:hover {
    background-color: rgba(255, 255, 255, 0.12);
}

.dock-button:active {
    background-color: rgba(255, 255, 255, 0.20);
}

.running-dot {
    min-width: 5px;
    min-height: 5px;
    border-radius: 5px;
    background-color: #ffffff;
    margin-top: 2px;
}

.dock-separator {
    min-width: 1px;
    background-color: rgba(255, 255, 255, 0.17);
    margin: 6px 4px;
}

popover contents {
    background-color: #1c1c24;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 10px;
    padding: 4px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.5);
}

.popover-box {
    padding: 2px;
}

.menu-header {
    color: #8a8a9e;
    font-size: 11px;
    font-weight: bold;
    padding: 4px 8px;
    text-transform: uppercase;
}

.menu-item-row {
    padding: 6px 8px;
    border-radius: 6px;
    color: #e0e0e0;
    transition: background-color 100ms ease;
}

.menu-item-row:hover {
    background-color: rgba(255, 255, 255, 0.08);
}

.popover-separator {
    background-color: rgba(255, 255, 255, 0.1);
    margin: 4px 0;
    min-height: 1px;
}

.win-label {
    font-family: monospace;
    font-size: 12px;
    color: #d0d0d8;
}

.close-btn-label {
    color: #ff5555;
    font-weight: bold;
    padding: 0 4px;
    border-radius: 4px;
}

.close-btn-label:hover {
    background-color: rgba(255, 85, 85, 0.25);
    color: #ff3333;
}

.add-label {
    font-size: 14px;
    font-weight: bold;
    color: #a0a0b0;
}
"#;

    if !css_path.exists() {
        let _ = fs::write(&css_path, default_css);
    }

    provider.load_from_path(&css_path);

    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let mut last_modified: Option<SystemTime> =
        fs::metadata(&css_path).and_then(|m| m.modified()).ok();

    let provider_clone = provider.clone();
    let css_path_clone = css_path.clone();

    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        if let Ok(metadata) = fs::metadata(&css_path_clone) {
            if let Ok(modified) = metadata.modified() {
                if last_modified != Some(modified) {
                    last_modified = Some(modified);
                    provider_clone.load_from_path(&css_path_clone);
                    println!("css reloaded");
                }
            }
        }
        glib::ControlFlow::Continue
    });
}
