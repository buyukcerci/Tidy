mod database;
mod keychain;
mod oauth;

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager, State};

use database::{AccountRecord, AccountState};
use oauth::{OAuthError, OAUTH_SERVICE};

pub struct AuthState {
    cancel: Arc<AtomicBool>,
    access_token: Mutex<Option<AccessTokenEntry>>,
}

struct AccessTokenEntry {
    // The access token is held only in memory and read by the future Drive
    // layer; nothing here stores or returns it to the frontend.
    #[allow(dead_code)]
    token: String,
    expires_at: Instant,
}

#[derive(serde::Serialize)]
struct HealthStatus {
    app: &'static str,
    database: database::DatabaseStatus,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionState {
    NoAccount,
    Disconnected,
    Connected,
}

#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
enum AuthenticationStatus {
    Idle,
    Connected,
    TemporarilyUnavailable,
    ReauthenticationRequired,
}

#[derive(serde::Serialize)]
struct ConnectionStatus {
    oauth_configured: bool,
    state: ConnectionState,
    account: Option<AccountRecord>,
    authentication: AuthenticationStatus,
    auth_message: Option<String>,
}

pub(crate) enum TokenStatus {
    Ready,
    RequiresReauthentication(String),
    TemporarilyUnavailable(String),
}

#[derive(serde::Serialize)]
struct DisconnectOutcome {
    revoked: bool,
    keychain_cleared: bool,
}

#[tauri::command]
fn health_check(app: AppHandle) -> Result<HealthStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let database = database::initialize(&data_dir)?;

    Ok(HealthStatus {
        app: "ok",
        database,
    })
}

#[tauri::command]
fn get_connection_status(app: AppHandle) -> Result<ConnectionStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let oauth_configured = database::get_oauth_config(&data_dir)?.is_some();
    let mut auth_message: Option<String> = None;

    let (state, account, authentication) = match database::get_account_state(&data_dir)? {
        AccountState::NoAccount => (ConnectionState::NoAccount, None, AuthenticationStatus::Idle),
        AccountState::Disconnected(record) => {
            let authentication = if record.auth_state == "reauthentication_required" {
                auth_message = record.auth_message.clone();
                AuthenticationStatus::ReauthenticationRequired
            } else {
                AuthenticationStatus::Idle
            };
            (ConnectionState::Disconnected, Some(record), authentication)
        }
        AccountState::Connected(record) => match access_token(&app)? {
            TokenStatus::Ready => (
                ConnectionState::Connected,
                Some(record),
                AuthenticationStatus::Connected,
            ),
            TokenStatus::RequiresReauthentication(message) => {
                // Missing or rejected credentials cannot authenticate Drive calls.
                // Mark the account disconnected now; keep the record so the UI can
                // explain what happened.
                database::mark_account_reauthentication_required(&data_dir, &message)?;
                auth_message = Some(message);
                (
                    ConnectionState::Disconnected,
                    Some(record),
                    AuthenticationStatus::ReauthenticationRequired,
                )
            }
            TokenStatus::TemporarilyUnavailable(message) => {
                auth_message = Some(message);
                (
                    ConnectionState::Connected,
                    Some(record),
                    AuthenticationStatus::TemporarilyUnavailable,
                )
            }
        },
    };

    Ok(ConnectionStatus {
        oauth_configured,
        state,
        account,
        authentication,
        auth_message,
    })
}

#[tauri::command]
fn save_oauth_configuration(
    app: AppHandle,
    client_id: String,
    client_secret: String,
) -> Result<(), String> {
    let client_id = client_id.trim();
    let client_secret = client_secret.trim();
    if client_id.len() < 5 || client_secret.is_empty() {
        return Err("The provided OAuth client details are incomplete.".to_string());
    }

    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    database::save_oauth_config(&data_dir, client_id, client_secret)
}

#[tauri::command]
async fn begin_google_connect(
    app: AppHandle,
    state: State<'_, AuthState>,
) -> Result<AccountRecord, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let config = database::get_oauth_config(&data_dir)?.ok_or_else(|| {
        "Google OAuth is not configured yet. Add your OAuth client first.".to_string()
    })?;

    state.cancel.store(false, Ordering::SeqCst);
    *state
        .access_token
        .lock()
        .map_err(|error| error.to_string())? = None;

    let cancel = state.cancel.clone();
    let flow = tauri::async_runtime::spawn_blocking(move || oauth::run_flow(&config, cancel));

    let result = match flow.await {
        Ok(outcome) => outcome.map_err(oauth_error_message)?,
        Err(join_error) => return Err(format!("sign-in task failed: {join_error}")),
    };

    let username = oauth::keychain_username(&result.identity.subject_id);
    keychain::store_refresh_token(OAUTH_SERVICE, &username, &result.refresh_token)
        .map_err(|error| error.to_string())?;

    *state
        .access_token
        .lock()
        .map_err(|error| error.to_string())? = Some(AccessTokenEntry {
        token: result.access_token,
        expires_at: Instant::now()
            + Duration::from_secs(result.access_token_expires_in.saturating_sub(30)),
    });

    let account = AccountRecord {
        subject_id: result.identity.subject_id,
        email: result.identity.email,
        display_name: result.identity.display_name,
        status: "connected".to_string(),
        auth_state: "connected".to_string(),
        auth_message: None,
    };
    database::set_account_connected(
        &data_dir,
        &account.subject_id,
        account.email.clone(),
        account.display_name.clone(),
    )?;

    Ok(account)
}

#[tauri::command]
fn cancel_google_connect(state: State<'_, AuthState>) -> Result<(), String> {
    state.cancel.store(true, Ordering::SeqCst);
    Ok(())
}

#[tauri::command]
async fn disconnect_google_account(
    app: AppHandle,
    state: State<'_, AuthState>,
    erase_local_data: bool,
) -> Result<DisconnectOutcome, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;

    let subject_id = match database::get_account_state(&data_dir)? {
        AccountState::Connected(record) => record.subject_id,
        AccountState::Disconnected(_) => {
            let _ = *state
                .access_token
                .lock()
                .map_err(|error| error.to_string())? = None;
            return Ok(DisconnectOutcome {
                revoked: true,
                keychain_cleared: true,
            });
        }
        AccountState::NoAccount => return Err("No account is connected.".to_string()),
    };

    let username = oauth::keychain_username(&subject_id);
    let mut revoked = false;
    if let Ok(Some(token)) = keychain::load_refresh_token(OAUTH_SERVICE, &username) {
        revoked = oauth::revoke_token(&token).is_ok();
    }

    let keychain_cleared = keychain::delete_refresh_token(OAUTH_SERVICE, &username).is_ok();

    *state
        .access_token
        .lock()
        .map_err(|error| error.to_string())? = None;

    if erase_local_data {
        database::erase_local_data(&data_dir)?;
    } else {
        database::mark_account_disconnected(&data_dir)?;
    }

    Ok(DisconnectOutcome {
        revoked,
        keychain_cleared,
    })
}

pub(crate) fn access_token(app: &AppHandle) -> Result<TokenStatus, String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| error.to_string())?;
    let config = database::get_oauth_config(&data_dir)?;

    let state = app.state::<AuthState>();
    {
        let cached = state
            .access_token
            .lock()
            .map_err(|error| error.to_string())?;
        if let Some(entry) = cached.as_ref() {
            if entry.expires_at > Instant::now() {
                return Ok(TokenStatus::Ready);
            }
        }
    }

    let config = match config {
        Some(config) => config,
        None => {
            return Ok(TokenStatus::RequiresReauthentication(
                "Google sign-in is not configured.".to_string(),
            ))
        }
    };

    let subject_id = match database::get_account_state(&data_dir)? {
        AccountState::Connected(record) => record.subject_id,
        _ => {
            return Ok(TokenStatus::RequiresReauthentication(
                "No connected account.".to_string(),
            ))
        }
    };

    let username = oauth::keychain_username(&subject_id);
    let refresh_token = match keychain::load_refresh_token(OAUTH_SERVICE, &username) {
        Ok(Some(token)) => token,
        Ok(None) => {
            return Ok(TokenStatus::RequiresReauthentication(
                "Stored sign-in credentials are missing. Please reconnect.".to_string(),
            ))
        }
        Err(error) => return Ok(TokenStatus::TemporarilyUnavailable(error.to_string())),
    };

    match oauth::refresh_access_token(&config, &refresh_token) {
        Ok((token, expires_in)) => {
            *state
                .access_token
                .lock()
                .map_err(|error| error.to_string())? = Some(AccessTokenEntry {
                token,
                expires_at: Instant::now() + Duration::from_secs(expires_in.saturating_sub(30)),
            });
            Ok(TokenStatus::Ready)
        }
        Err(OAuthError::TokenResponse(error)) if error.requires_reauthentication() => {
            Ok(TokenStatus::RequiresReauthentication(format!(
                "Google rejected the stored sign-in: {}",
                error.message
            )))
        }
        Err(OAuthError::TokenResponse(error)) => Ok(TokenStatus::TemporarilyUnavailable(format!(
            "Google sign-in is temporarily unavailable: {}",
            error.message
        ))),
        Err(error) => Ok(TokenStatus::TemporarilyUnavailable(error.to_string())),
    }
}

fn oauth_error_message(error: OAuthError) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AuthState {
            cancel: Arc::new(AtomicBool::new(false)),
            access_token: Mutex::new(None),
        })
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app.path().app_data_dir().map_err(|error| {
                std::io::Error::new(std::io::ErrorKind::Other, error.to_string())
            })?;
            database::initialize(&data_dir)?;

            let window = app.get_webview_window("main").ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::NotFound, "main window not found")
            })?;
            let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))?;
            window.set_icon(icon)?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            health_check,
            get_connection_status,
            save_oauth_configuration,
            begin_google_connect,
            cancel_google_connect,
            disconnect_google_account
        ])
        .run(tauri::generate_context!())
        .expect("error while running Tidy application");
}
