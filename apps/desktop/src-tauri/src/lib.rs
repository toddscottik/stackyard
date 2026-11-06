mod commands;
mod folder;
mod open;

use tauri::menu::{Menu, MenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Manager, WindowEvent};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            commands::engine_status,
            commands::probe_engine,
            folder::pick_project_parent,
            commands::list_recipes,
            commands::inspect_config,
            commands::preview_config,
            commands::apply_config,
            commands::reload_project,
            commands::forget_project,
            commands::plan_create,
            commands::apply_create,
            commands::record_create_runtime,
            commands::engine_setting,
            commands::put_setting,
            commands::list_packages,
            commands::list_catalog,
            commands::start_install,
            commands::install_status,
            commands::cancel_install,
            commands::use_existing,
            commands::select_package,
            commands::select_scope_package,
            commands::preview_project,
            commands::start_dependency_scoped,
            commands::php_status_scoped,
            commands::php_apply_profile_scoped,
            commands::php_install_extension_scoped,
            commands::list_projects,
            commands::import_project,
            commands::project_status,
            commands::start_project,
            commands::stop_project,
            commands::restart_project,
            commands::cancel_start,
            commands::read_logs,
            commands::follow_logs,
            commands::list_activity,
            commands::start_dependency,
            commands::task_status,
            commands::cancel_task,
            commands::open_project,
            commands::front_status,
            commands::publish_project,
            commands::unpublish_project,
            commands::restart_front,
            commands::open_site,
            commands::databases_status,
            commands::database_create,
            commands::database_start,
            commands::database_stop,
            commands::database_attach,
            commands::database_detach,
            commands::sqlite_use,
            commands::list_updates,
            commands::cleanup_unused,
            commands::export_diagnostics,
            commands::discover_runtimes,
            commands::php_status,
            commands::php_apply_profile,
            commands::php_install_extension,
            commands::memcached_probe,
            commands::caches_status,
            commands::cache_create,
            commands::cache_start,
            commands::cache_stop,
            commands::cache_forget
        ])
        .setup(|app| {
            let show = MenuItem::with_id(app, "show", "Show Stackyard", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show, &quit])?;
            let mut tray = TrayIconBuilder::with_id("main")
                .menu(&menu)
                .tooltip("Stackyard")
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "show" => commands::show_main(app),
                    "quit" => {
                        let handle = app.clone();
                        tauri::async_runtime::spawn(async move {
                            commands::quit_from_tray(&handle).await;
                        });
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        commands::show_main(tray.app_handle());
                    }
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let hidden = window.clone();
                window.on_window_event(move |event| {
                    if let WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = hidden.hide();
                    }
                });
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running Stackyard");
}
