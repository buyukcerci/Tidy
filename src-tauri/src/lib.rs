mod database;

use tauri::{image::Image, AppHandle, Manager};

#[derive(serde::Serialize)]
struct HealthStatus {
    app: &'static str,
    database: database::DatabaseStatus,
}

#[tauri::command]
fn health_check(app: AppHandle) -> Result<HealthStatus, String> {
    let data_dir = app.path().app_data_dir().map_err(|error| error.to_string())?;
    let database = database::initialize(&data_dir)?;

    Ok(HealthStatus {
        app: "ok",
        database,
    })
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let window = app.get_webview_window("main").ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "main window not found")
            })?;
            let icon = Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            window.set_icon(icon)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![health_check])
        .run(tauri::generate_context!())
        .expect("error while running Tidy application");
}
