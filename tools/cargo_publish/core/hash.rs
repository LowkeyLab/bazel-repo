use sha2::Digest;

use crate::ContractError;

/// Lowercase hexadecimal SHA256 fingerprint of actual bytes.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256(String);

impl Sha256 {
    /// Construct a canonical lowercase hexadecimal fingerprint.
    ///
    /// # Errors
    /// Returns a fingerprint error unless the value contains exactly 64 lowercase
    /// hexadecimal ASCII characters.
    pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
        let value = value.into();
        if value.len() != 64
            || !value
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(ContractError::Sha256(value));
        }
        Ok(Self(value))
    }
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Sha256 {
    type Error = ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<Sha256> for String {
    fn from(value: Sha256) -> Self {
        value.0
    }
}

impl std::fmt::Display for Sha256 {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[must_use]
pub fn digest_bytes(bytes: &[u8]) -> Sha256 {
    fingerprint(&sha2::Sha256::digest(bytes))
}

fn fingerprint(bytes: &[u8]) -> Sha256 {
    let mut hexadecimal = String::with_capacity(64);
    for byte in bytes {
        for nibble in [byte >> 4, byte & 0x0f] {
            hexadecimal.push(char::from(b"0123456789abcdef"[usize::from(nibble)]));
        }
    }
    Sha256(hexadecimal)
}

/// Fingerprint file bytes, independent of timestamps and filesystem metadata.
///
/// # Errors
/// Returns an I/O error when the file cannot be read.
pub fn digest_file(path: impl AsRef<std::path::Path>) -> Result<Sha256, ContractError> {
    use std::io::Read;
    let path = path.as_ref();
    let error = |source| ContractError::Io {
        path: path.to_owned(),
        source,
    };
    let mut file = std::fs::File::open(path).map_err(error)?;
    let mut digest = sha2::Sha256::new();
    let mut buffer = [0; 8 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(error)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(fingerprint(&digest.finalize()))
}
