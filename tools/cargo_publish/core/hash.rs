use sha2::Digest;

use crate::ContractError;

/// Lowercase hexadecimal SHA256 fingerprint of actual bytes.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sha256(String);

impl Sha256 {
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

pub fn digest_bytes(bytes: &[u8]) -> Sha256 {
    Sha256(
        sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    )
}

/// Fingerprint file bytes, independent of timestamps and filesystem metadata.
pub fn digest_file(path: impl AsRef<std::path::Path>) -> Result<Sha256, ContractError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })?;
    Ok(digest_bytes(&bytes))
}
