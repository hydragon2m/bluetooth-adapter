mod monitor;
mod providers;

use monitor::{Monitor, MonitorSettings, MonitorSnapshot};
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    AppHandle, Manager, State,
};

#[tauri::command]
fn get_snapshot(monitor: State<'_, Monitor>) -> Result<MonitorSnapshot, String> {
    monitor.snapshot()
}

#[tauri::command]
async fn refresh_devices(
    app: AppHandle,
    monitor: State<'_, Monitor>,
) -> Result<MonitorSnapshot, String> {
    let monitor = monitor.inner().clone();
    tauri::async_runtime::spawn_blocking(move || monitor.refresh(&app))
        .await
        .map_err(|error| format!("Device scan could not finish: {error}"))?
}

#[tauri::command]
async fn update_settings(
    app: AppHandle,
    monitor: State<'_, Monitor>,
    settings: MonitorSettings,
) -> Result<MonitorSnapshot, String> {
    let monitor = monitor.inner().clone();
    tauri::async_runtime::spawn_blocking(move || monitor.update_settings(&app, settings))
        .await
        .map_err(|error| format!("Settings could not be saved: {error}"))?
}

fn create_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // libappindicator-sys panics if neither shared library exists. Detect this
    // before entering the tray implementation so the window still works.
    #[cfg(target_os = "linux")]
    {
        let libraries = ["libayatana-appindicator3.so.1", "libappindicator3.so.1"];
        let available = libraries.iter().any(|name| {
            // SAFETY: These are the same fixed system libraries loaded by the
            // tray dependency; no symbols or borrowed values escape this call.
            unsafe { libloading::Library::new(name) }.is_ok()
        });
        if !available {
            return Err("Không tìm thấy thư viện AppIndicator cho khay hệ thống.".into());
        }
    }
    let show = MenuItem::with_id(app, "show", "Mở Device Battery", true, None::<&str>)?;
    let hide = MenuItem::with_id(
        app,
        "hide",
        "Ẩn cửa sổ · tiếp tục theo dõi",
        true,
        None::<&str>,
    )?;
    let quit = MenuItem::with_id(app, "quit", "Thoát", true, None::<&str>)?;
    let menu = Menu::with_items(app, &[&show, &hide, &quit])?;
    let mut tray = TrayIconBuilder::with_id("device-battery")
        .tooltip("Device Battery")
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id.as_ref() {
            "show" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.unminimize();
                    let _ = window.set_focus();
                }
            }
            "hide" => {
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.hide();
                }
            }
            "quit" => app.exit(0),
            _ => {}
        });
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    tray.build(app)?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let monitor = Monitor::new(app.handle());
            match create_tray(app.handle()) {
                Ok(()) => monitor.set_tray_available(true),
                Err(error) => eprintln!("Device Battery: system tray unavailable: {error}"),
            }
            app.manage(monitor.clone());
            monitor.start_polling(app.handle().clone());
            Ok(())
        })
        // Closing the window quits normally. Background mode is only entered
        // through the tray menu that the user can also use to restore it.
        .invoke_handler(tauri::generate_handler![
            get_snapshot,
            refresh_devices,
            update_settings
        ])
        .run(tauri::generate_context!())
        .expect("error while running Device Battery");
}
