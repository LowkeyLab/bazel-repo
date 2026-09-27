pub mod error;
pub mod identity;

pub use error::IdentityError;
pub use identity::{Book, BookId, BookRegistry, EventId, OpenLibraryWorkId, ReaderId};

#[cfg(test)]
mod tests {
    use googletest::prelude::*;
    use uuid::Uuid;

    use super::{Book, BookId, BookRegistry, IdentityError, OpenLibraryWorkId};

    fn book(id: u128, work_id: &str) -> Book {
        Book::new(
            BookId::new(Uuid::from_u128(id)),
            OpenLibraryWorkId::try_from(work_id).expect("valid work ID"),
        )
    }

    #[googletest::test]
    fn register_book_is_idempotent() {
        let mut registry = BookRegistry::new();
        let original = book(1, "OL45804W");

        assert_that!(registry.register(original.clone()), eq(&Ok(())));
        assert_that!(registry.register(original.clone()), eq(&Ok(())));
        assert_that!(registry.get(original.id()), eq(Some(&original)));
    }

    #[googletest::test]
    fn registration_rejects_conflicting_identity() {
        let mut registry = BookRegistry::new();
        let original = book(1, "OL45804W");
        registry
            .register(original.clone())
            .expect("first registration");

        assert_that!(
            registry.register(book(2, "OL45804W")),
            eq(&Err(IdentityError::WorkIdConflict))
        );
        assert_that!(
            registry.register(book(1, "OL45805W")),
            eq(&Err(IdentityError::BookIdConflict))
        );
        assert_that!(registry.get(original.id()), eq(Some(&original)));
        assert_that!(registry.get(BookId::new(Uuid::from_u128(2))), eq(None));
    }

    #[googletest::test]
    fn work_id_requires_canonical_work_form() {
        let valid = OpenLibraryWorkId::try_from("OL45804W").expect("canonical work ID");
        assert_that!(valid.as_str(), eq("OL45804W"));

        for invalid in [
            "OL7353617M",
            "/works/OL45804W",
            "OLW",
            "OL0W",
            "OL045804W",
            "OL٤٥٨٠٤W",
        ] {
            assert_that!(OpenLibraryWorkId::try_from(invalid).is_err(), eq(true));
        }
    }
}
