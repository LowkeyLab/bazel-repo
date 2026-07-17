use std::collections::HashSet;
use std::fs::File;
use std::io::{Cursor, Read, Seek, Write};
use std::path::{Component, Path};

use tempfile::NamedTempFile;
use zip::CompressionMethod;
use zip::read::ZipArchive;

use crate::ImportLimits;
use crate::cancellation::ImportCancellation;
use crate::error::{ImportError, ImportErrorCode, Result};

pub(crate) struct CollectionArchive {
    pub database: NamedTempFile,
    pub database_name: String,
}

pub(crate) fn extract_collection(
    bytes: &[u8],
    extension: &str,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<CollectionArchive> {
    cancellation.check()?;
    if bytes.len() as u64 > limits.max_upload_bytes {
        return Err(ImportError::new(ImportErrorCode::UploadTooLarge));
    }
    extract_collection_from_reader(Cursor::new(bytes), extension, limits, cancellation)
}

pub(crate) fn extract_collection_file(
    path: &Path,
    extension: &str,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<CollectionArchive> {
    cancellation.check()?;
    let file = File::open(path).map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?;
    let size = file
        .metadata()
        .map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?
        .len();
    if size > limits.max_upload_bytes {
        return Err(ImportError::new(ImportErrorCode::UploadTooLarge));
    }
    extract_collection_from_reader(file, extension, limits, cancellation)
}

fn extract_collection_from_reader<R: Read + Seek>(
    reader: R,
    extension: &str,
    limits: &ImportLimits,
    cancellation: &ImportCancellation,
) -> Result<CollectionArchive> {
    cancellation.check()?;
    if extension != ".apkg" {
        return Err(ImportError::new(ImportErrorCode::UnsupportedExtension));
    }

    let mut archive =
        ZipArchive::new(reader).map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?;
    if archive.len() > limits.max_archive_entries {
        return Err(ImportError::new(ImportErrorCode::EntryCountExceeded));
    }

    let mut database_index = None;
    let mut database_name = None;
    let mut names = HashSet::new();
    let mut total = 0_u64;
    for index in 0..archive.len() {
        cancellation.check()?;
        let file = archive
            .by_index(index)
            .map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?;
        let name = file.name().to_owned();
        validate_name(&name)?;
        if !names.insert(name.clone()) {
            return Err(ImportError::new(ImportErrorCode::InvalidZip));
        }
        if file.is_dir()
            || !matches!(
                file.compression(),
                CompressionMethod::Stored | CompressionMethod::Deflated
            )
        {
            return Err(ImportError::new(ImportErrorCode::InvalidZip));
        }
        if let Some(mode) = file.unix_mode() {
            let kind = mode & 0o170000;
            if kind != 0 && kind != 0o100000 {
                return Err(ImportError::new(ImportErrorCode::InvalidZip));
            }
        }
        if file.size() > limits.max_entry_bytes {
            return Err(ImportError::new(ImportErrorCode::EntrySizeExceeded));
        }
        total = total
            .checked_add(file.size())
            .ok_or_else(|| ImportError::new(ImportErrorCode::TotalExtractedSizeExceeded))?;
        if total > limits.max_extracted_bytes {
            return Err(ImportError::new(
                ImportErrorCode::TotalExtractedSizeExceeded,
            ));
        }
        let ratio = file.size() / file.compressed_size().max(1);
        if ratio > limits.max_compression_ratio {
            return Err(ImportError::new(ImportErrorCode::CompressionRatioExceeded));
        }
        match name.as_str() {
            "collection.anki2" | "collection.anki21" => {
                if database_index.replace(index).is_some() {
                    return Err(ImportError::new(ImportErrorCode::InvalidZip));
                }
                database_name = Some(name);
            }
            "media" => {}
            _ => return Err(ImportError::new(ImportErrorCode::MediaRejected)),
        }
    }

    let index = database_index
        .ok_or_else(|| ImportError::new(ImportErrorCode::MissingCollectionDatabase))?;
    if let Ok(mut media) = archive.by_name("media") {
        cancellation.check()?;
        let mut contents = String::new();
        media
            .read_to_string(&mut contents)
            .map_err(|_| ImportError::new(ImportErrorCode::MediaRejected))?;
        cancellation.check()?;
        let value: serde_json::Value = serde_json::from_str(&contents)
            .map_err(|_| ImportError::new(ImportErrorCode::MediaRejected))?;
        if !value.as_object().is_some_and(serde_json::Map::is_empty) {
            return Err(ImportError::new(ImportErrorCode::MediaRejected));
        }
    }

    let mut source = archive
        .by_index(index)
        .map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?;
    let expected_size = source.size();
    let mut database =
        NamedTempFile::new().map_err(|_| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
    let mut copied = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        cancellation.check()?;
        let read = source
            .read(&mut buffer)
            .map_err(|_| ImportError::new(ImportErrorCode::InvalidZip))?;
        if read == 0 {
            break;
        }
        database
            .write_all(&buffer[..read])
            .map_err(|_| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
        copied = copied.saturating_add(read as u64);
    }
    if copied != expected_size {
        return Err(ImportError::new(ImportErrorCode::InvalidZip));
    }
    database
        .flush()
        .map_err(|_| ImportError::new(ImportErrorCode::SqliteSchemaInvalid))?;
    cancellation.check()?;

    Ok(CollectionArchive {
        database,
        database_name: database_name.expect("database name is set with its index"),
    })
}

fn validate_name(name: &str) -> Result<()> {
    if name.is_empty()
        || name.contains('\\')
        || name.contains('\0')
        || name.as_bytes().get(1) == Some(&b':')
    {
        return Err(ImportError::new(ImportErrorCode::InvalidZip));
    }
    let path = std::path::Path::new(name);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir
                    | Component::CurDir
                    | Component::RootDir
                    | Component::Prefix(_)
            )
        })
        || name.split('/').any(str::is_empty)
    {
        return Err(ImportError::new(ImportErrorCode::InvalidZip));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unsafe_archive_paths() {
        for path in ["../collection.anki2", "/collection.anki2", "a\\b", "a//b"] {
            assert_eq!(
                validate_name(path)
                    .expect_err("unsafe name is rejected")
                    .code,
                ImportErrorCode::InvalidZip
            );
        }
    }

    #[test]
    fn accepts_only_normal_relative_names() {
        validate_name("collection.anki21").expect("supported relative entry is accepted");
    }
}
