use keyring::Entry;

#[derive(Debug)]
pub enum KeychainError {
    Store(String),
    Load(String),
    Delete(String),
}

impl std::fmt::Display for KeychainError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::Store(kind) => format!("could not store credentials ({kind})"),
            Self::Load(kind) => format!("could not read credentials ({kind})"),
            Self::Delete(kind) => format!("could not remove credentials ({kind})"),
        };
        formatter.write_str(&message)
    }
}

impl std::error::Error for KeychainError {}

fn describe(error: &keyring::Error) -> String {
    match error {
        keyring::Error::NoEntry => "no entry present".to_string(),
        keyring::Error::NoStorageAccess(error) => format!("keychain unavailable: {error}"),
        keyring::Error::PlatformFailure(error) => {
            format!("operating-system keychain failure: {error}")
        }
        other => format!("keychain error: {other}"),
    }
}

pub fn store_refresh_token(
    service: &str,
    username: &str,
    refresh_token: &str,
) -> Result<(), KeychainError> {
    Entry::new(service, username)
        .and_then(|entry| entry.set_password(refresh_token))
        .map_err(|error| KeychainError::Store(describe(&error)))
}

pub fn load_refresh_token(service: &str, username: &str) -> Result<Option<String>, KeychainError> {
    match Entry::new(service, username).and_then(|entry| entry.get_password()) {
        Ok(token) => Ok(Some(token)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(KeychainError::Load(describe(&error))),
    }
}

pub fn delete_refresh_token(service: &str, username: &str) -> Result<(), KeychainError> {
    if load_refresh_token(service, username)?.is_none() {
        return Ok(());
    }
    Entry::new(service, username)
        .and_then(|entry| entry.delete_credential())
        .map_err(|error| KeychainError::Delete(describe(&error)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describe_maps_common_errors_without_leaking_secrets() {
        assert!(!describe(&keyring::Error::NoEntry).is_empty());
        let message = format!(
            "{}",
            KeychainError::Load(describe(&keyring::Error::NoEntry))
        );
        assert!(message.contains("credentials"));
    }
}
