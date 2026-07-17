use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportErrorCode {
    UploadTooLarge,
    UnsupportedExtension,
    InvalidZip,
    EntryCountExceeded,
    EntrySizeExceeded,
    TotalExtractedSizeExceeded,
    CompressionRatioExceeded,
    MissingCollectionDatabase,
    UnsupportedPackageVersion,
    SqliteLimitExceeded,
    SqliteSchemaInvalid,
    NoSupportedNotes,
    TooManyCards,
    ClozeRejected,
    MediaRejected,
    CustomTemplateRejected,
    InvalidUtf8,
    EmptyFront,
    EmptyBack,
    Cancelled,
}

#[derive(Debug, Error)]
#[error("APKG import rejected: {code:?}")]
pub struct ImportError {
    pub code: ImportErrorCode,
}

impl ImportError {
    pub(crate) const fn new(code: ImportErrorCode) -> Self {
        Self { code }
    }
}

pub(crate) type Result<T> = std::result::Result<T, ImportError>;
