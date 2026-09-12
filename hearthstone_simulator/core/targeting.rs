#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TargetAudience {
    Friendly,
    Enemy,
    Either,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TargetKind {
    Minion,
    Hero,
    Character,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct TargetFilter {
    pub audience: TargetAudience,
    pub kind: TargetKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Deserialize, serde::Serialize)]
pub enum TargetRequirement {
    None,
    Required(TargetFilter),
    Optional(TargetFilter),
    RequiredIfAvailable(TargetFilter),
}
