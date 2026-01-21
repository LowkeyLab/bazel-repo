use book::BookId;
use uuid;

pub struct User {
    pub id: uuid::Uuid,
    pub username: String,
    pub books: Vec<BookId>,
}

impl Default for User {
    fn default() -> Self {
        User {
            id: uuid::Uuid::new_v4(),
            username: String::from("Unknown User"),
            books: Vec::new(),
        }
    }
}

impl User {
    pub fn new(id: uuid::Uuid, username: String) -> Self {
        User {
            id,
            username,
            books: Vec::new(),
        }
    }
}
