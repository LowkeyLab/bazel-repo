use crate::ContractError;

/// Canonical POSIX-relative package path, independent of the host filesystem.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, serde::Serialize, serde::Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct PackagePath(String);

impl PackagePath {
    pub fn new(value: impl Into<String>) -> Result<Self, ContractError> {
        let value = value.into();
        if value.is_empty()
            || value.starts_with('/')
            || value.contains(['\\', '\0'])
            || value.split('/').any(|part| matches!(part, "" | "." | ".."))
        {
            return Err(ContractError::PackagePath(value));
        }
        Ok(Self(value))
    }

    /// Construct a declared source/asset path, reserving Cargo-generated files.
    pub fn declared(value: impl Into<String>) -> Result<Self, ContractError> {
        let path = Self::new(value)?;
        if matches!(
            path.as_str(),
            "Cargo.toml" | "Cargo.toml.orig" | "Cargo.lock" | ".cargo_vcs_info.json"
        ) {
            return Err(ContractError::Policy(format!(
                "conflicting declared package path: {path}"
            )));
        }
        Ok(path)
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for PackagePath {
    type Error = ContractError;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl From<PackagePath> for String {
    fn from(value: PackagePath) -> Self {
        value.0
    }
}

impl std::fmt::Display for PackagePath {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}
