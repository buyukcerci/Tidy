use rusqlite::Connection;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(serde::Serialize)]
pub struct DatabaseStatus {
    pub status: &'static str,
    pub path: String,
    pub schema_version: i64,
}

pub fn initialize(data_dir: &Path) -> Result<DatabaseStatus, String> {
    fs::create_dir_all(data_dir).map_err(|error| error.to_string())?;
    let database_path = data_dir.join("tidy.sqlite3");
    let connection = Connection::open(&database_path).map_err(|error| error.to_string())?;

    connection
        .execute_batch(include_str!("../migrations/001_initial.sql"))
        .map_err(|error| error.to_string())?;

    let schema_version: i64 = connection
        .query_row(
            "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
            [],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;

    Ok(DatabaseStatus {
        status: "ok",
        path: display_path(database_path),
        schema_version,
    })
}

fn display_path(path: PathBuf) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::initialize;
    use std::fs;

    #[test]
    fn initializes_database_from_empty_directory() {
        let data_dir = std::env::temp_dir().join(format!(
            "tidy-test-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&data_dir);

        let result = initialize(&data_dir).expect("database should initialize");

        assert_eq!(result.status, "ok");
        assert_eq!(result.schema_version, 1);
        assert!(data_dir.join("tidy.sqlite3").exists());

        fs::remove_dir_all(data_dir).expect("test database should be removable");
    }
}
