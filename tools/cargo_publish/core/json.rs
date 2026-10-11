use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::ContractError;

/// Read a fixed typed JSON contract, retaining the input path in diagnostics.
pub fn read_json<T: DeserializeOwned>(path: impl AsRef<Path>) -> Result<T, ContractError> {
    let path = path.as_ref();
    let bytes = std::fs::read(path).map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| ContractError::Json {
        path: path.to_owned(),
        source,
    })
}

/// Write the existing readable schema representation with a trailing newline.
pub fn write_json<T: Serialize>(path: impl AsRef<Path>, value: &T) -> Result<(), ContractError> {
    let path = path.as_ref();
    let mut bytes = serde_json::to_vec_pretty(value).map_err(|source| ContractError::Json {
        path: path.to_owned(),
        source,
    })?;
    bytes.push(b'\n');
    std::fs::write(path, bytes).map_err(|source| ContractError::Io {
        path: path.to_owned(),
        source,
    })
}
