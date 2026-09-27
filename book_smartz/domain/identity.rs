use std::collections::HashMap;

use uuid::Uuid;

use crate::IdentityError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BookId(Uuid);

impl BookId {
    #[must_use]
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ReaderId(Uuid);

impl ReaderId {
    #[must_use]
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct EventId(Uuid);

impl EventId {
    #[must_use]
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    #[must_use]
    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct OpenLibraryWorkId(String);

impl OpenLibraryWorkId {
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for OpenLibraryWorkId {
    type Error = IdentityError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let digits = value
            .strip_prefix("OL")
            .and_then(|value| value.strip_suffix('W'))
            .ok_or(IdentityError::InvalidWorkId)?;
        if digits.is_empty()
            || digits.as_bytes()[0] == b'0'
            || !digits.bytes().all(|digit| digit.is_ascii_digit())
        {
            return Err(IdentityError::InvalidWorkId);
        }
        Ok(Self(value.to_owned()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Book {
    id: BookId,
    work_id: OpenLibraryWorkId,
}

impl Book {
    #[must_use]
    pub fn new(id: BookId, work_id: OpenLibraryWorkId) -> Self {
        Self { id, work_id }
    }

    #[must_use]
    pub fn id(&self) -> BookId {
        self.id
    }

    #[must_use]
    pub fn work_id(&self) -> &OpenLibraryWorkId {
        &self.work_id
    }
}

#[derive(Default)]
pub struct BookRegistry {
    by_id: HashMap<BookId, Book>,
    by_work_id: HashMap<OpenLibraryWorkId, BookId>,
}

impl BookRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds an identity mapping, accepting a repeat of the identical mapping.
    ///
    /// # Errors
    /// Returns an identity conflict if either ID already maps to a different value.
    pub fn register(&mut self, book: Book) -> Result<(), IdentityError> {
        if let Some(existing) = self.by_id.get(&book.id) {
            return if existing == &book {
                Ok(())
            } else {
                Err(IdentityError::BookIdConflict)
            };
        }
        if self.by_work_id.contains_key(&book.work_id) {
            return Err(IdentityError::WorkIdConflict);
        }
        self.by_work_id.insert(book.work_id.clone(), book.id);
        self.by_id.insert(book.id, book);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: BookId) -> Option<&Book> {
        self.by_id.get(&id)
    }
}
