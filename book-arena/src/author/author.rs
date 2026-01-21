use uuid;

#[derive(Debug, Clone, PartialEq, Hash, Default)]
pub struct AuthorId(uuid::Uuid);

impl AuthorId {
    pub fn new() -> Self {
        AuthorId(uuid::Uuid::new_v4())
    }
}

impl From<uuid::Uuid> for AuthorId {
    fn from(id: uuid::Uuid) -> Self {
        AuthorId(id)
    }
}

#[derive(Debug, Clone, PartialEq, Hash, Default)]
pub struct Author {
    pub id: AuthorId,
    pub name: String,
}

impl Author {
    pub fn new() -> Self {
        Default::default()
    }
}
