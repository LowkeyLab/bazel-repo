#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum IdentityError {
    #[error("invalid Open Library work ID")]
    InvalidWorkId,
    #[error("book ID already refers to a different work")]
    BookIdConflict,
    #[error("work ID already refers to a different book")]
    WorkIdConflict,
}
