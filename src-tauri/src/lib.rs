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
        .on_tray_icon_event(|tray, event| match event {
            tauri::tray::TrayIconEvent::Click { button_state, rect, .. } => {
                if button_state == tauri::tray::MouseButtonState::Up {
                    if let Some(window) = tray.app_handle().get_webview_window("tray") {
                        if window.is_visible().unwrap_or(false) {
                            let _ = window.hide();
                        } else {
                            let (x, y) = match rect.position {
                                tauri::Position::Physical(p) => (p.x, p.y),
                                tauri::Position::Logical(l) => (l.x as i32, l.y as i32),
                            };
                            let (width, height) = match rect.size {
                                tauri::Size::Physical(p) => (p.width, p.height),
                                tauri::Size::Logical(l) => (l.width as u32, l.height as u32),
                            };
                            let target_x = x - 150 + (width as i32 / 2);
                            let target_y = y - 410;
                            
                            let final_y = if target_y < 0 { y + height as i32 + 10 } else { target_y };
                            let final_x = if target_x < 0 { 10 } else { target_x };
                            
                            let _ = window.set_position(tauri::Position::Physical(tauri::PhysicalPosition::new(final_x, final_y)));
                            let _ = window.show();
                            let _ = window.set_focus();
                        }
                    }
                }
            }
            tauri::tray::TrayIconEvent::Leave { .. } => {
                if let Some(window) = tray.app_handle().get_webview_window("tray") {
                    let _ = window.hide();
                }
            }
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
            update_settings,
            open_main,
            quit_app
        ])
        .run(tauri::generate_context!())
        .expect("error while running Device Battery");
}

#[tauri::command]
fn open_main(app: tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}
