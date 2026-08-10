use rusqlite::{params, Connection};
use std::fs;
use std::path::{Path, PathBuf};

use crate::oauth::OauthConfig;

#[derive(serde::Serialize)]
pub struct DatabaseStatus {
    pub status: &'static str,
    pub path: String,
    pub schema_version: i64,
}

const MIGRATIONS: [(i64, &str); 2] = [
    (2, include_str!("../migrations/002_account.sql")),
    (3, include_str!("../migrations/003_auth_state.sql")),
];

const OAUTH_CLIENT_ID_KEY: &str = "google_oauth_client_id";
const OAUTH_CLIENT_SECRET_KEY: &str = "google_oauth_client_secret";

#[derive(Debug)]
pub enum AccountState {
    NoAccount,
    Disconnected(AccountRecord),
    Connected(AccountRecord),
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AccountRecord {
    pub subject_id: String,
    pub email: Option<String>,
    pub display_name: Option<String>,
    pub status: String,
    pub auth_state: String,
    pub auth_message: Option<String>,
}

pub fn connect(data_dir: &Path) -> Result<Connection, String> {
    fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let database_path = data_dir.join("tidy.sqlite3");
    let connection = Connection::open(database_path).map_err(|error| error.to_string())?;
    apply_migrations(&connection)?;
    Ok(connection)
}

fn apply_migrations(connection: &Connection) -> Result<(), String> {
    // Migration 001 creates the version table and is intentionally idempotent.
    connection
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .map_err(|error| error.to_string())?;

    for (version, migration) in MIGRATIONS {
        let applied: bool = connection
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
                [version],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        if !applied {
            connection
                .execute_batch(migration)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn initialize(data_dir: &Path) -> Result<DatabaseStatus, String> {
    let connection = connect(data_dir)?;

    let schema_version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(DatabaseStatus {
        status: "ok",
        path: display_path(data_dir.join("tidy.sqlite3")),
        schema_version,
    })
}

fn display_path(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

fn read_account(connection: &Connection) -> Result<Option<AccountRecord>, String> {
    connection
        .query_row(
            "SELECT subject_id, email, display_name, status, auth_state, auth_message
             FROM account WHERE id = 1",
            [],
            |row| {
                Ok(AccountRecord {
                    subject_id: row.get(0)?,
                    email: row.get(1)?,
                    display_name: row.get(2)?,
                    status: row.get(3)?,
                    auth_state: row.get(4)?,
                    auth_message: row.get(5)?,
                })
            },
        )
        .map(Some)
        .or_else(|error| {
            if error == rusqlite::Error::QueryReturnedNoRows {
                Ok(None)
            } else {
                Err(error.to_string())
            }
        })
}

pub fn get_account_state(data_dir: &Path) -> Result<AccountState, String> {
    let connection = connect(data_dir)?;
    match read_account(&connection)? {
        None => Ok(AccountState::NoAccount),
        Some(account) if account.status == "connected" => Ok(AccountState::Connected(account)),
        Some(account) => Ok(AccountState::Disconnected(account)),
    }
}

pub fn set_account_connected(
    data_dir: &Path,
    subject_id: &str,
    email: Option<String>,
    display_name: Option<String>,
) -> Result<(), String> {
    let connection = connect(data_dir)?;
    connection
        .execute(
            "INSERT INTO account
                 (id, subject_id, email, display_name, status, auth_state, auth_message)
             VALUES (1, ?1, ?2, ?3, 'connected', 'connected', NULL)
             ON CONFLICT(id) DO UPDATE SET
                 subject_id = excluded.subject_id,
                 email = excluded.email,
                 display_name = excluded.display_name,
                 status = 'connected',
                 auth_state = 'connected',
                 auth_message = NULL,
                 updated_at = CURRENT_TIMESTAMP",
            params![subject_id, email, display_name],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn mark_account_disconnected(data_dir: &Path) -> Result<(), String> {
    let connection = connect(data_dir)?;
    connection
        .execute(
            "UPDATE account
             SET status = 'disconnected', auth_state = 'idle', auth_message = NULL,
                 updated_at = CURRENT_TIMESTAMP
             WHERE id = 1",
            [],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn mark_account_reauthentication_required(
    data_dir: &Path,
    message: &str,
) -> Result<(), String> {
    let connection = connect(data_dir)?;
    connection
        .execute(
            "UPDATE account
             SET status = 'disconnected', auth_state = 'reauthentication_required',
                 auth_message = ?1, updated_at = CURRENT_TIMESTAMP
             WHERE id = 1",
            [message],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn erase_local_data(data_dir: &Path) -> Result<(), String> {
    let connection = connect(data_dir)?;
    connection
        .execute("DELETE FROM account WHERE id = 1", [])
        .map_err(|error| error.to_string())?;
    connection
        .execute_batch(
            "DROP TABLE IF EXISTS files;
         DROP TABLE IF EXISTS file_parents;
         DROP TABLE IF EXISTS scan_runs;",
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

pub fn get_oauth_config(data_dir: &Path) -> Result<Option<OauthConfig>, String> {
    let connection = connect(data_dir)?;
    let client_id: Option<String> = read_metadata(&connection, OAUTH_CLIENT_ID_KEY)?;
    let client_secret: Option<String> = read_metadata(&connection, OAUTH_CLIENT_SECRET_KEY)?;
    match (client_id, client_secret) {
        (Some(client_id), Some(client_secret)) => Ok(Some(OauthConfig {
            client_id,
            client_secret,
        })),
        _ => Ok(None),
    }
}

pub fn save_oauth_config(
    data_dir: &Path,
    client_id: &str,
    client_secret: &str,
) -> Result<(), String> {
    let connection = connect(data_dir)?;
    write_metadata(&connection, OAUTH_CLIENT_ID_KEY, client_id)?;
    write_metadata(&connection, OAUTH_CLIENT_SECRET_KEY, client_secret)?;
    Ok(())
}

fn read_metadata(connection: &Connection, key: &str) -> Result<Option<String>, String> {
    connection
        .query_row(
            "SELECT value FROM app_metadata WHERE key = ?1",
            [key],
            |row| row.get(0),
        )
        .map(Some)
        .or_else(|error| {
            if error == rusqlite::Error::QueryReturnedNoRows {
                Ok(None)
            } else {
                Err(error.to_string())
            }
        })
}

fn write_metadata(connection: &Connection, key: &str, value: &str) -> Result<(), String> {
    connection
        .execute(
            "INSERT INTO app_metadata (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_data_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tidy-test-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn initializes_database_from_empty_directory() {
        let data_dir = temp_data_dir("init");
        let result = initialize(&data_dir).expect("database should initialize");

        assert_eq!(result.status, "ok");
        assert_eq!(result.schema_version, 3);
        assert!(data_dir.join("tidy.sqlite3").exists());

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn entry_points_migrate_before_querying_fresh_directory() {
        let data_dir = temp_data_dir("fresh-query");
        // No explicit initialize call: every entry point must apply migrations
        // first so the frontend cannot race schema creation with queries.
        assert!(matches!(
            get_account_state(&data_dir).expect("state should read"),
            AccountState::NoAccount
        ));
        assert!(get_oauth_config(&data_dir)
            .expect("config should read")
            .is_none());

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn account_lifecycle_connected_then_disconnected() {
        let data_dir = temp_data_dir("account");
        initialize(&data_dir).expect("database should initialize");

        let state = get_account_state(&data_dir).expect("state should read");
        assert!(matches!(state, AccountState::NoAccount));

        set_account_connected(
            &data_dir,
            "subject-1",
            Some("user@example.com".to_string()),
            Some("Example User".to_string()),
        )
        .expect("account should connect");

        match get_account_state(&data_dir).expect("state should read") {
            AccountState::Connected(account) => {
                assert_eq!(account.subject_id, "subject-1");
                assert_eq!(account.email, Some("user@example.com".to_string()));
                assert_eq!(account.display_name, Some("Example User".to_string()));
                assert_eq!(account.auth_state, "connected");
                assert_eq!(account.auth_message, None);
            }
            other => panic!("expected connected account, got {other:?}"),
        }

        mark_account_disconnected(&data_dir).expect("account should disconnect");
        match get_account_state(&data_dir).expect("state should read") {
            AccountState::Disconnected(record) => assert_eq!(record.subject_id, "subject-1"),
            other => panic!("expected disconnected account, got {other:?}"),
        }
        let connection = connect(&data_dir).unwrap();
        let record = read_account(&connection).unwrap().unwrap();
        assert_eq!(record.subject_id, "subject-1");

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn reconnecting_overwrites_single_account_row() {
        let data_dir = temp_data_dir("reconnect");
        initialize(&data_dir).expect("database should initialize");

        set_account_connected(&data_dir, "first", None, None).expect("first should connect");
        set_account_connected(&data_dir, "second", None, None).expect("second should connect");

        let connection = connect(&data_dir).unwrap();
        let count: i64 = connection
            .query_row("SELECT COUNT(*) FROM account WHERE id = 1", [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 1);
        let record = read_account(&connection).unwrap().unwrap();
        assert_eq!(record.subject_id, "second");

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn erase_local_data_removes_account() {
        let data_dir = temp_data_dir("erase");
        initialize(&data_dir).expect("database should initialize");
        set_account_connected(&data_dir, "subject-1", None, None).expect("should connect");
        assert!(matches!(
            get_account_state(&data_dir).expect("state should read"),
            AccountState::Connected(_)
        ));

        erase_local_data(&data_dir).expect("local data should erase");
        assert!(matches!(
            get_account_state(&data_dir).expect("state should read"),
            AccountState::NoAccount
        ));

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn oauth_config_round_trip() {
        let data_dir = temp_data_dir("config");
        initialize(&data_dir).expect("database should initialize");

        assert!(get_oauth_config(&data_dir)
            .expect("config should read")
            .is_none());

        save_oauth_config(
            &data_dir,
            "client-id.apps.googleusercontent.com",
            "client-secret",
        )
        .expect("config should save");

        let config = get_oauth_config(&data_dir)
            .expect("config should read")
            .expect("config present");
        assert_eq!(config.client_id, "client-id.apps.googleusercontent.com");
        assert_eq!(config.client_secret, "client-secret");

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn reauthentication_reason_persists_across_reads() {
        let data_dir = temp_data_dir("reauth");
        set_account_connected(&data_dir, "subject-1", None, None).expect("should connect");
        mark_account_reauthentication_required(&data_dir, "Credentials were revoked.")
            .expect("state should update");

        for _ in 0..2 {
            match get_account_state(&data_dir).expect("state should read") {
                AccountState::Disconnected(account) => {
                    assert_eq!(account.auth_state, "reauthentication_required");
                    assert_eq!(
                        account.auth_message.as_deref(),
                        Some("Credentials were revoked.")
                    );
                }
                other => panic!("expected reauthentication state, got {other:?}"),
            }
        }

        set_account_connected(&data_dir, "subject-1", None, None)
            .expect("reconnect should succeed");
        match get_account_state(&data_dir).expect("state should read") {
            AccountState::Connected(account) => {
                assert_eq!(account.auth_state, "connected");
                assert_eq!(account.auth_message, None);
            }
            other => panic!("expected connected account, got {other:?}"),
        }

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }

    #[test]
    fn upgrades_existing_version_two_database() {
        let data_dir = temp_data_dir("upgrade-v2");
        fs::create_dir_all(&data_dir).expect("directory should exist");
        let connection = Connection::open(data_dir.join("tidy.sqlite3")).unwrap();
        connection
            .execute_batch(include_str!("../migrations/001_initial.sql"))
            .unwrap();
        connection
            .execute_batch(include_str!("../migrations/002_account.sql"))
            .unwrap();
        drop(connection);

        let status = initialize(&data_dir).expect("database should upgrade");
        assert_eq!(status.schema_version, 3);
        set_account_connected(&data_dir, "subject-1", None, None).expect("account should save");

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }
}
