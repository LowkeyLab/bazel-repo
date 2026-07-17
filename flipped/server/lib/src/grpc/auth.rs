use tonic::Status;
use tonic::metadata::{AsciiMetadataValue, MetadataMap};

pub fn bearer(metadata: &MetadataMap) -> Result<&str, Status> {
    let mut values = metadata.get_all("authorization").iter();
    let value = values
        .next()
        .ok_or_else(|| Status::unauthenticated("missing bearer credential"))?;
    if values.next().is_some() {
        return Err(Status::unauthenticated("duplicate bearer credential"));
    }
    bearer_value(value)
}

fn bearer_value(value: &AsciiMetadataValue) -> Result<&str, Status> {
    let value = value
        .to_str()
        .map_err(|_| Status::unauthenticated("invalid bearer credential"))?;
    let token = value
        .strip_prefix("Bearer ")
        .filter(|token| !token.is_empty() && !token.contains(char::is_whitespace))
        .ok_or_else(|| Status::unauthenticated("invalid bearer credential"))?;
    Ok(token)
}

pub fn reject_authorization(metadata: &MetadataMap) -> Result<(), Status> {
    if metadata.contains_key("authorization") {
        Err(Status::unauthenticated(
            "CreateSession does not accept authorization metadata",
        ))
    } else {
        Ok(())
    }
}
