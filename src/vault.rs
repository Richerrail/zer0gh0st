use anyhow::{anyhow, Result};
use base64::Engine;
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use crate::config;

#[derive(Serialize, Deserialize)]
struct VaultFile {
    salt: String,
    nonce: String,
    data: String,
}

pub fn path() -> PathBuf {
    config::config_dir().join("vault.enc")
}

pub fn enabled() -> bool {
    path().exists()
}

fn unlocked() -> &'static Mutex<HashMap<String, String>> {
    static U: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    U.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn get_unlocked(provider: &str) -> Option<String> {
    unlocked().lock().ok()?.get(provider).cloned()
}

pub fn is_unlocked() -> bool {
    unlocked().lock().map(|m| !m.is_empty()).unwrap_or(false)
}

pub fn lock() {
    if let Ok(mut m) = unlocked().lock() {
        m.clear();
    }
}

fn derive_key(passphrase: &str, salt: &[u8]) -> Result<[u8; 32]> {
    let mut key = [0u8; 32];
    argon2::Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| anyhow!("argon2: {e}"))?;
    Ok(key)
}

fn encrypt(passphrase: &str, plaintext: &[u8]) -> Result<VaultFile> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    rand::thread_rng().fill_bytes(&mut salt);
    rand::thread_rng().fill_bytes(&mut nonce);

    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    let ct = cipher
        .encrypt(Nonce::from_slice(&nonce), plaintext)
        .map_err(|e| anyhow!("chiffrement: {e}"))?;

    Ok(VaultFile {
        salt: b64.encode(salt),
        nonce: b64.encode(nonce),
        data: b64.encode(ct),
    })
}

fn decrypt(passphrase: &str, v: &VaultFile) -> Result<Vec<u8>> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let salt = b64.decode(&v.salt)?;
    let nonce = b64.decode(&v.nonce)?;
    let ct = b64.decode(&v.data)?;

    let key = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key));
    cipher
        .decrypt(Nonce::from_slice(&nonce), ct.as_ref())
        .map_err(|_| anyhow!("passphrase incorrecte ou coffre corrompu"))
}

pub fn setup(passphrase: &str) -> Result<usize> {
    if passphrase.is_empty() {
        return Err(anyhow!("passphrase vide"));
    }
    let mut g = config::load_global();
    let keys: HashMap<String, String> =
        g.keys.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
    let plaintext = serde_json::to_vec(&keys)?;
    let v = encrypt(passphrase, &plaintext)?;
    write_file(&v)?;

    let n = keys.len();
    if let Ok(mut m) = unlocked().lock() {
        *m = keys;
    }
    g.keys.clear();
    config::save_global(&g)?;
    Ok(n)
}

pub fn unlock(passphrase: &str) -> Result<usize> {
    let raw = std::fs::read_to_string(path())?;
    let v: VaultFile = serde_json::from_str(&raw)?;
    let plaintext = decrypt(passphrase, &v)?;
    let keys: HashMap<String, String> = serde_json::from_slice(&plaintext)?;
    let n = keys.len();
    if let Ok(mut m) = unlocked().lock() {
        *m = keys;
    }
    Ok(n)
}

pub fn put(key: &str, value: &str) -> Result<()> {
    if !enabled() {
        let mut keys: HashMap<String, String> = HashMap::new();
        keys.insert(key.to_string(), value.to_string());
        let plaintext = serde_json::to_vec(&keys)?;
        let v = encrypt(&passphrase()?, &plaintext)?;
        write_file(&v)?;
        if let Ok(mut m) = unlocked().lock() {
            *m = keys;
        }
        return Ok(());
    }
    let mut keys: HashMap<String, String> = {
        let m = unlocked().lock().map_err(|_| anyhow!("cofre verrouillé"))?;
        m.clone()
    };
    keys.insert(key.to_string(), value.to_string());
    let plaintext = serde_json::to_vec(&keys)?;
    let v = encrypt(&passphrase()?, &plaintext)?;
    write_file(&v)?;
    if let Ok(mut m) = unlocked().lock() {
        m.insert(key.to_string(), value.to_string());
    }
    Ok(())
}

fn write_file(v: &VaultFile) -> Result<()> {
    std::fs::write(path(), serde_json::to_string_pretty(v)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(path(), std::fs::Permissions::from_mode(0o600));
    }
    Ok(())
}

pub fn remove(key: &str) -> Result<bool> {
    if !enabled() {
        return Ok(false);
    }
    let mut keys: HashMap<String, String> = {
        let m = unlocked().lock().map_err(|_| anyhow!("cofre verrouillé"))?;
        m.clone()
    };
    let existed = keys.remove(key).is_some();
    let plaintext = serde_json::to_vec(&keys)?;
    let v = encrypt(&passphrase()?, &plaintext)?;
    write_file(&v)?;
    if let Ok(mut m) = unlocked().lock() {
        m.remove(key);
    }
    Ok(existed)
}

fn passphrase() -> Result<String> {
    if let Ok(p) = std::env::var("ZER0_VAULT_PASSPHRASE") {
        if !p.is_empty() {
            return Ok(p);
        }
    }
    if !std::io::stdin().is_terminal() {
        return Err(anyhow!(
            "passphrase du coffre inconnue : définis ZER0_VAULT_PASSPHRASE (ou utilise `zer0 vault unlock` en CLI)"
        ));
    }
    let p = rpassword::prompt_password("Zer0 — passphrase du coffre : ")?;
    if p.is_empty() {
        return Err(anyhow!("passphrase vide"));
    }
    Ok(p)
}

pub fn reset() -> Result<()> {
    if enabled() {
        std::fs::remove_file(path())?;
    }
    lock();
    Ok(())
}

pub fn try_env_unlock() -> Result<bool> {
    if !enabled() || is_unlocked() {
        return Ok(true);
    }
    if let Ok(p) = std::env::var("ZER0_VAULT_PASSPHRASE") {
        if !p.is_empty() {
            unlock(&p)?;
            return Ok(true);
        }
    }
    Ok(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stored_secret_is_never_in_cleartext_on_disk() {
        let dir = std::env::temp_dir().join("zer0-vault-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("vault.enc");

        let secret = "ghp_EXEMPLE_DE_VRAI_JETON_1234567890";
        let mut keys: HashMap<String, String> = HashMap::new();
        keys.insert("github".to_string(), secret.to_string());
        let plaintext = serde_json::to_vec(&keys).unwrap();
        let v = encrypt("passphrase-de-test", &plaintext).unwrap();
        std::fs::write(&file, serde_json::to_string_pretty(&v).unwrap()).unwrap();

        let raw = std::fs::read_to_string(&file).unwrap();
        assert!(
            !raw.contains(secret),
            "le secret apparaît en clair dans le fichier du coffre !"
        );
        assert!(
            !raw.contains("ghp_"),
            "le secret apparaît en clair dans le coffre !"
        );

        let v2: VaultFile = serde_json::from_str(&raw).unwrap();
        let dec = decrypt("passphrase-de-test", &v2).unwrap();
        let back: HashMap<String, String> = serde_json::from_slice(&dec).unwrap();
        assert_eq!(back.get("github").map(String::as_str), Some(secret));

        assert!(decrypt("mauvaise-passphrase", &v2).is_err());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn setup_accepts_empty_config() {
        let empty: HashMap<String, String> = HashMap::new();
        let plaintext = serde_json::to_vec(&empty).unwrap();
        let v = encrypt("passphrase-de-test", &plaintext).unwrap();
        let dec = decrypt("passphrase-de-test", &v).unwrap();
        let back: HashMap<String, String> = serde_json::from_slice(&dec).unwrap();
        assert!(back.is_empty());
    }
}
