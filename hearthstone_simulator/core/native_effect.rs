#[derive(
    Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, serde::Deserialize, serde::Serialize,
)]
pub struct NativeEffectId(pub String);

impl NativeEffectId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
}

impl From<&str> for NativeEffectId {
    fn from(id: &str) -> Self {
        Self::new(id)
    }
}

#[cfg(test)]
mod tests {
    use googletest::prelude::*;

    use super::*;

    #[googletest::test]
    fn native_effect_ids_convert_from_string_slices() {
        let id: NativeEffectId = "synthetic:test".into();

        assert_that!(id, eq(&NativeEffectId::new("synthetic:test")));
    }
}
