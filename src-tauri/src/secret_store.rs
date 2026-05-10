//! Secure local secret storage for Narrative.
//!
//! Uses the OS keychain (macOS Keychain / Windows Credential Manager / Secret Service)
//! via the `keyring` crate.

use rand::RngCore;

const SERVICE: &str = "com.jamie.trace-narrative";
const LEGACY_SERVICE: &str = "com.jamie.narrative-mvp";
const OTLP_KEY_USER: &str = "otlp_api_key";

fn entry_for(service: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new(service, OTLP_KEY_USER).map_err(|e| e.to_string())
}

fn entry() -> Result<keyring::Entry, String> {
    entry_for(SERVICE)
}

pub fn get_otlp_api_key() -> Result<Option<String>, String> {
    let primary = entry()?;
    match primary.get_password() {
        Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
        Ok(_) | Err(keyring::Error::NoEntry) => {
            let legacy = entry_for(LEGACY_SERVICE)?;
            match legacy.get_password() {
                Ok(value) if !value.trim().is_empty() => Ok(Some(value)),
                Ok(_) => Ok(None),
                Err(keyring::Error::NoEntry) => Ok(None),
                Err(err) => Err(err.to_string()),
            }
        }
        Err(err) => Err(err.to_string()),
    }
}

pub fn set_otlp_api_key(value: &str) -> Result<(), String> {
    if value.trim().is_empty() {
        return Err("API key cannot be empty".to_string());
    }
    let e = entry()?;
    e.set_password(value).map_err(|e| e.to_string())
}

pub fn delete_otlp_api_key() -> Result<(), String> {
    for service in [SERVICE, LEGACY_SERVICE] {
        let e = entry_for(service)?;
        match e.delete_password() {
            Ok(()) | Err(keyring::Error::NoEntry) => {}
            Err(err) => return Err(err.to_string()),
        }
    }
    Ok(())
}

pub fn generate_otlp_api_key_hex() -> String {
    // 24 bytes => 48 hex chars
    let mut bytes = [0u8; 24];
    rand::rng().fill_bytes(&mut bytes);
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

pub fn ensure_otlp_api_key() -> Result<String, String> {
    let primary = entry()?;
    match primary.get_password() {
        Ok(value) if !value.trim().is_empty() => return Ok(value),
        Ok(_) | Err(keyring::Error::NoEntry) => {}
        Err(err) => return Err(err.to_string()),
    }

    // Migrate legacy keychain service entry when present.
    let legacy = entry_for(LEGACY_SERVICE)?;
    match legacy.get_password() {
        Ok(value) if !value.trim().is_empty() => {
            set_otlp_api_key(&value)?;
            return Ok(value);
        }
        Ok(_) | Err(keyring::Error::NoEntry) => {}
        Err(err) => return Err(err.to_string()),
    }

    let key = generate_otlp_api_key_hex();
    set_otlp_api_key(&key)?;
    Ok(key)
}

pub fn masked_preview(value: &str) -> String {
    let v = value.trim();
    if v.len() <= 8 {
        return "********".to_string();
    }
    format!("{}…{}", &v[..4], &v[v.len() - 4..])
}
