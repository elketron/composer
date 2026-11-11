use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

use crate::devices::{Device, PairingCode};
use crate::error::{Result, ServerError};
use crate::hash::content_hash;
use crate::secret::secret_eq;

/// How long a pairing code works.
const CODE_MINUTES: i64 = 10;
/// Wrong codes tried while a code is open, after which that code stops working.
const MAX_WRONG_CODES: u32 = 5;
/// Codes avoid look-alike characters (no 0/O, 1/I/L).
const CODE_ALPHABET: &[u8] = b"23456789ABCDEFGHJKMNPQRSTUVWXYZ";

/// Paired devices and open pairing codes, kept in `state/` as files so the CLI
/// (`composer pair`, `composer devices`) and a running server share them.
#[derive(Debug)]
pub struct Devices {
    devices_file: PathBuf,
    codes_file: PathBuf,
    /// Devices as last read, with the file's modification time.
    cache: Mutex<(Option<SystemTime>, Vec<Device>)>,
    /// One redeem at a time, so parallel guesses are all counted.
    redeeming: Mutex<()>,
}

impl Devices {
    pub fn new(state: &Path) -> Self {
        Self {
            devices_file: state.join("devices.json"),
            codes_file: state.join("pairing.json"),
            cache: Mutex::new((None, Vec::new())),
            redeeming: Mutex::new(()),
        }
    }

    /// The name of the device a token belongs to.
    pub fn verify(&self, token: &str) -> Option<String> {
        self.find(token).map(|d| d.name)
    }

    /// The device a token belongs to. Every device is compared, each in constant time.
    pub fn find(&self, token: &str) -> Option<Device> {
        let hash = content_hash(token.as_bytes());
        let mut found = None;
        for device in self.devices() {
            if secret_eq(&device.token_hash, &hash) {
                found = Some(device);
            }
        }
        found
    }

    /// Whether the device whose token hashes to `token_hash` is (still) paired.
    pub fn is_paired(&self, token_hash: &str) -> bool {
        self.devices().iter().any(|d| d.token_hash == token_hash)
    }

    pub fn devices(&self) -> Vec<Device> {
        let modified = std::fs::metadata(&self.devices_file)
            .and_then(|m| m.modified())
            .ok();
        let mut cache = self.cache.lock().expect("devices lock");
        if cache.0 != modified || modified.is_none() {
            *cache = (modified, read_json(&self.devices_file).unwrap_or_default());
        }
        cache.1.clone()
    }

    /// Opens a pairing code for a device called `name` (formatted `ABCD-EFGH`).
    pub fn create_code(&self, name: &str) -> Result<String> {
        let raw: String = (0..8)
            .map(|_| CODE_ALPHABET[rand::random_range(0..CODE_ALPHABET.len())] as char)
            .collect();
        let code = format!("{}-{}", &raw[..4], &raw[4..]);
        let now = chrono::Utc::now();
        let mut codes: Vec<PairingCode> = read_json(&self.codes_file).unwrap_or_default();
        codes.retain(|c| chrono::DateTime::parse_from_rfc3339(&c.expires).is_ok_and(|e| e > now));
        codes.push(PairingCode {
            code_hash: hash_code(&code),
            name: name.to_string(),
            expires: (now + chrono::Duration::minutes(CODE_MINUTES)).to_rfc3339(),
            failures: 0,
        });
        write_json(&self.codes_file, &codes)?;
        Ok(code)
    }

    /// Trades a valid, unexpired code for a new device token (once). The device is
    /// named `name`, or what the code was created with.
    ///
    /// A wrong code counts against every code that is open; one that saw [`MAX_WRONG_CODES`] of
    /// them stops working (`composer pair` makes a new one), so a code cannot be guessed at.
    pub fn redeem(&self, code: &str, name: Option<&str>) -> Result<Option<(String, String)>> {
        let _one_at_a_time = self.redeeming.lock().expect("redeem lock");
        let now = chrono::Utc::now();
        let mut codes: Vec<PairingCode> = read_json(&self.codes_file).unwrap_or_default();
        codes.retain(|c| chrono::DateTime::parse_from_rfc3339(&c.expires).is_ok_and(|e| e > now));
        let hash = hash_code(code);
        // Every open code is compared, each in constant time.
        let mut position = None;
        for (at, open) in codes.iter().enumerate() {
            if secret_eq(&open.code_hash, &hash) {
                position = Some(at);
            }
        }
        let Some(position) = position else {
            if codes.is_empty() {
                return Ok(None);
            }
            for open in &mut codes {
                open.failures += 1;
            }
            for spent in codes.iter().filter(|c| c.failures >= MAX_WRONG_CODES) {
                tracing::warn!(
                    "pairing: {MAX_WRONG_CODES} wrong codes were tried; the open code for `{}` \
                     no longer works, run `composer pair` for a new one",
                    spent.name
                );
            }
            codes.retain(|c| c.failures < MAX_WRONG_CODES);
            write_json(&self.codes_file, &codes)?;
            return Ok(None);
        };
        let pairing = codes.remove(position);
        write_json(&self.codes_file, &codes)?;

        let mut devices = self.devices();
        let base = name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or(&pairing.name)
            .trim()
            .to_string();
        // Names are how devices are revoked, so keep them unique.
        let mut device_name = base.clone();
        let mut n = 2;
        while devices.iter().any(|d| d.name == device_name) {
            device_name = format!("{base}-{n}");
            n += 1;
        }
        let token: String = (0..4)
            .map(|_| format!("{:016x}", rand::random::<u64>()))
            .collect();
        devices.push(Device {
            name: device_name.clone(),
            token_hash: content_hash(token.as_bytes()),
            paired_at: now.to_rfc3339(),
        });
        write_json(&self.devices_file, &devices)?;
        Ok(Some((device_name, token)))
    }

    /// Removes a device; its token stops working immediately.
    pub fn revoke(&self, name: &str) -> Result<bool> {
        let mut devices = self.devices();
        let before = devices.len();
        devices.retain(|d| d.name != name);
        if devices.len() == before {
            return Ok(false);
        }
        write_json(&self.devices_file, &devices)?;
        Ok(true)
    }
}

/// Codes are compared without the dash and case-insensitively (easier to type).
fn hash_code(code: &str) -> String {
    let normalized: String = code
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>()
        .to_uppercase();
    content_hash(normalized.as_bytes())
}

fn read_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

fn write_json<T: serde::Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| ServerError::io(parent, e))?;
    }
    let json = serde_json::to_string_pretty(value).expect("serializes");
    std::fs::write(path, json).map_err(|e| ServerError::io(path, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pair_once_verify_revoke() {
        let dir = tempfile::tempdir().unwrap();
        let devices = Devices::new(dir.path());
        let code = devices.create_code("phone").unwrap();
        assert_eq!(code.len(), 9);
        assert!(devices.redeem("WRONG-CODE", None).unwrap().is_none());

        let (name, token) = devices
            .redeem(&code.to_lowercase().replace('-', ""), None)
            .unwrap()
            .unwrap();
        assert_eq!(name, "phone");
        assert!(
            devices.redeem(&code, None).unwrap().is_none(),
            "codes work once"
        );
        assert_eq!(devices.verify(&token).as_deref(), Some("phone"));
        assert!(devices.verify("nope").is_none());

        let second = devices.create_code("phone").unwrap();
        let (name, _) = devices.redeem(&second, None).unwrap().unwrap();
        assert_eq!(name, "phone-2", "names stay unique");

        assert!(devices.revoke("phone").unwrap());
        assert!(devices.verify(&token).is_none());
        assert!(
            !std::fs::read_to_string(dir.path().join("devices.json"))
                .unwrap()
                .contains(&token),
            "only hashes are stored"
        );
    }

    #[test]
    fn a_device_is_found_by_its_token_until_revoked() {
        let dir = tempfile::tempdir().unwrap();
        let devices = Devices::new(dir.path());
        let code = devices.create_code("phone").unwrap();
        let (_, token) = devices.redeem(&code, None).unwrap().unwrap();
        let device = devices.find(&token).unwrap();
        assert_eq!(device.name, "phone");
        assert!(devices.is_paired(&device.token_hash));
        assert!(devices.find("nope").is_none());
        devices.revoke("phone").unwrap();
        assert!(!devices.is_paired(&device.token_hash));
    }

    #[test]
    fn wrong_codes_use_up_the_open_code() {
        let dir = tempfile::tempdir().unwrap();
        let devices = Devices::new(dir.path());
        // Four wrong codes: the right one still works.
        let code = devices.create_code("phone").unwrap();
        for _ in 0..MAX_WRONG_CODES - 1 {
            assert!(devices.redeem("AAAA-AAAA", None).unwrap().is_none());
        }
        assert!(devices.redeem(&code, None).unwrap().is_some());
        // Five: it is gone, and a new one starts from zero.
        let code = devices.create_code("tablet").unwrap();
        for _ in 0..MAX_WRONG_CODES {
            assert!(devices.redeem("AAAA-AAAA", None).unwrap().is_none());
        }
        assert!(devices.redeem(&code, None).unwrap().is_none());
        let fresh = devices.create_code("tablet").unwrap();
        assert!(devices.redeem("AAAA-AAAA", None).unwrap().is_none());
        assert!(devices.redeem(&fresh, None).unwrap().is_some());
        // A code made while guesses were counted against an older one has its own count.
        let (old, new) = (
            devices.create_code("a").unwrap(),
            devices.create_code("b").unwrap(),
        );
        for _ in 0..MAX_WRONG_CODES - 1 {
            devices.redeem("AAAA-AAAA", None).unwrap();
        }
        let newest = devices.create_code("c").unwrap();
        devices.redeem("AAAA-AAAA", None).unwrap();
        assert!(devices.redeem(&old, None).unwrap().is_none());
        assert!(devices.redeem(&new, None).unwrap().is_none());
        assert!(devices.redeem(&newest, None).unwrap().is_some());
    }
}
