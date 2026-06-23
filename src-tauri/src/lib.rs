mod commands;
pub mod scanner;

use commands::AppState;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{Emitter, Manager};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            commands::scan_directory,
            commands::cancel_scan,
            commands::get_subtree,
            commands::get_home_directory,
            commands::get_common_directories,
            commands::validate_path,
            commands::delete_path,
            commands::delete_node,
            commands::open_in_finder,
        ])
        .setup(|app| {
            // ── File menu ────────────────────────────────────────────────
            let open_item = MenuItem::with_id(
                app, "open-folder", "Open Folder…", true, Some("CmdOrCtrl+O"),
            )?;
            let file_menu = Submenu::with_items(
                app, "File", true,
                &[
                    &open_item,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::close_window(app, None)?,
                ],
            )?;

            // ── View menu ────────────────────────────────────────────────
            let viz_treemap = MenuItem::with_id(
                app, "viz-treemap", "Treemap", true, None::<&str>,
            )?;
            let viz_sunburst = MenuItem::with_id(
                app, "viz-sunburst", "Sunburst", true, None::<&str>,
            )?;
            let view_menu = Submenu::with_items(
                app, "View", true,
                &[&viz_treemap, &viz_sunburst],
            )?;

            // ── Window / Theme submenu ───────────────────────────────────
            let theme_system    = MenuItem::with_id(app, "theme-system",    "System",    true, None::<&str>)?;
            let theme_latte     = MenuItem::with_id(app, "theme-latte",     "Latte",     true, None::<&str>)?;
            let theme_frappe    = MenuItem::with_id(app, "theme-frappe",    "Frappé",    true, None::<&str>)?;
            let theme_macchiato = MenuItem::with_id(app, "theme-macchiato", "Macchiato", true, None::<&str>)?;
            let theme_mocha     = MenuItem::with_id(app, "theme-mocha",     "Mocha",     true, None::<&str>)?;
            let theme_submenu = Submenu::with_items(
                app, "Theme", true,
                &[
                    &theme_system,
                    &PredefinedMenuItem::separator(app)?,
                    &theme_latte, &theme_frappe, &theme_macchiato, &theme_mocha,
                ],
            )?;

            // ── Window / Accent submenu ──────────────────────────────────
            let acc_rosewater = MenuItem::with_id(app, "accent-rosewater", "Rosewater", true, None::<&str>)?;
            let acc_flamingo  = MenuItem::with_id(app, "accent-flamingo",  "Flamingo",  true, None::<&str>)?;
            let acc_pink      = MenuItem::with_id(app, "accent-pink",      "Pink",      true, None::<&str>)?;
            let acc_mauve     = MenuItem::with_id(app, "accent-mauve",     "Mauve",     true, None::<&str>)?;
            let acc_red       = MenuItem::with_id(app, "accent-red",       "Red",       true, None::<&str>)?;
            let acc_maroon    = MenuItem::with_id(app, "accent-maroon",    "Maroon",    true, None::<&str>)?;
            let acc_peach     = MenuItem::with_id(app, "accent-peach",     "Peach",     true, None::<&str>)?;
            let acc_yellow    = MenuItem::with_id(app, "accent-yellow",    "Yellow",    true, None::<&str>)?;
            let acc_green     = MenuItem::with_id(app, "accent-green",     "Green",     true, None::<&str>)?;
            let acc_teal      = MenuItem::with_id(app, "accent-teal",      "Teal",      true, None::<&str>)?;
            let acc_sky       = MenuItem::with_id(app, "accent-sky",       "Sky",       true, None::<&str>)?;
            let acc_sapphire  = MenuItem::with_id(app, "accent-sapphire",  "Sapphire",  true, None::<&str>)?;
            let acc_blue      = MenuItem::with_id(app, "accent-blue",      "Blue",      true, None::<&str>)?;
            let acc_lavender  = MenuItem::with_id(app, "accent-lavender",  "Lavender",  true, None::<&str>)?;
            let accent_submenu = Submenu::with_items(
                app, "Accent Color", true,
                &[
                    &acc_rosewater, &acc_flamingo, &acc_pink, &acc_mauve,
                    &acc_red, &acc_maroon,
                    &PredefinedMenuItem::separator(app)?,
                    &acc_peach, &acc_yellow, &acc_green, &acc_teal,
                    &PredefinedMenuItem::separator(app)?,
                    &acc_sky, &acc_sapphire, &acc_blue, &acc_lavender,
                ],
            )?;

            let window_menu = Submenu::with_items(
                app, "Window", true,
                &[
                    &PredefinedMenuItem::minimize(app, None)?,
                    &PredefinedMenuItem::maximize(app, None)?,
                    &PredefinedMenuItem::fullscreen(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &theme_submenu,
                    &accent_submenu,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::bring_all_to_front(app, None)?,
                ],
            )?;

            // ── diskviz app menu ─────────────────────────────────────────
            let shortcuts_item = MenuItem::with_id(
                app, "show-shortcuts", "Keyboard Shortcuts", true, Some("CmdOrCtrl+Shift+/"),
            )?;
            let notices_item = MenuItem::with_id(
                app, "show-notices", "Open Source Notices", true, None::<&str>,
            )?;
            let app_menu = Submenu::with_items(
                app, "diskviz", true,
                &[
                    &PredefinedMenuItem::about(app, None, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &shortcuts_item,
                    &notices_item,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::services(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::hide(app, None)?,
                    &PredefinedMenuItem::hide_others(app, None)?,
                    &PredefinedMenuItem::show_all(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::quit(app, None)?,
                ],
            )?;

            // ── Edit menu ────────────────────────────────────────────────
            let edit_menu = Submenu::with_items(
                app, "Edit", true,
                &[
                    &PredefinedMenuItem::undo(app, None)?,
                    &PredefinedMenuItem::redo(app, None)?,
                    &PredefinedMenuItem::separator(app)?,
                    &PredefinedMenuItem::cut(app, None)?,
                    &PredefinedMenuItem::copy(app, None)?,
                    &PredefinedMenuItem::paste(app, None)?,
                    &PredefinedMenuItem::select_all(app, None)?,
                ],
            )?;

            // ── Assemble & install ───────────────────────────────────────
            let menu = Menu::with_items(
                app,
                &[&app_menu, &file_menu, &edit_menu, &view_menu, &window_menu],
            )?;
            app.set_menu(menu)?;

            // ── Route menu events to the frontend ────────────────────────
            app.on_menu_event(|app, event| {
                let Some(window) = app.get_webview_window("main") else { return };
                match event.id().as_ref() {
                    "open-folder"    => { let _ = window.emit("menu-open-folder", ()); }
                    "show-shortcuts" => { let _ = window.emit("menu-show-shortcuts", ()); }
                    "show-notices"   => { let _ = window.emit("menu-show-notices", ()); }

                    // View
                    "viz-treemap"  => { let _ = window.emit("menu-set-visualization", "treemap"); }
                    "viz-sunburst" => { let _ = window.emit("menu-set-visualization", "sunburst"); }

                    // Theme
                    id if id.starts_with("theme-") => {
                        let value = &id["theme-".len()..];
                        let _ = window.emit("menu-set-theme", value);
                    }

                    // Accent
                    id if id.starts_with("accent-") => {
                        let value = &id["accent-".len()..];
                        let _ = window.emit("menu-set-accent", value);
                    }

                    _ => {}
                }
            });

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
