//! Semantic values used throughout the bot. Wrappers distinguish identities and
//! units; command and event validation retain responsibility for valid values.
//! Serde is transparent here. Boundary-specific Discord string encodings remain
//! on the event fields, while announcement snapshots retain numeric IDs.
use serde::{Deserialize, Serialize};

macro_rules! scalar {
    ($name:ident, $inner:ty) => {
        #[derive(
            Clone,
            Copy,
            Debug,
            Default,
            PartialEq,
            Eq,
            PartialOrd,
            Ord,
            Hash,
            Serialize,
            Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub $inner);

        impl From<$inner> for $name {
            fn from(value: $inner) -> Self {
                Self(value)
            }
        }
        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                self.0.fmt(f)
            }
        }
        impl std::str::FromStr for $name {
            type Err = <$inner as std::str::FromStr>::Err;
            fn from_str(value: &str) -> Result<Self, Self::Err> {
                value.parse().map(Self)
            }
        }
    };
}

scalar!(UserId, u64);
scalar!(GuildId, u64);
scalar!(ApplicationId, u64);
scalar!(ChannelId, u64);
scalar!(Points, i64);
scalar!(OutcomeIndex, usize);
scalar!(EventRevision, i64);
scalar!(ConfigurationVersion, i64);

impl EventRevision {
    #[must_use]
    pub fn next(self) -> Option<Self> {
        self.0.checked_add(1).map(Self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct MarketId(pub String);

impl From<String> for MarketId {
    fn from(value: String) -> Self {
        Self(value)
    }
}
impl From<&str> for MarketId {
    fn from(value: &str) -> Self {
        Self(value.to_owned())
    }
}
impl std::fmt::Display for MarketId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}
impl std::borrow::Borrow<str> for MarketId {
    fn borrow(&self) -> &str {
        &self.0
    }
}
