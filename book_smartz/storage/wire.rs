use crate::StoreError;

pub fn sequence_key(value: u64) -> Result<String, StoreError> {
    if value == 0 {
        return Err(StoreError::CorruptHistory("sequence is zero"));
    }
    Ok(format!("{value:020}"))
}

pub fn parse_sequence_key(value: &str) -> Result<u64, StoreError> {
    if value.len() != 20 || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(StoreError::CorruptHistory("invalid sequence key"));
    }
    let parsed = value
        .parse::<u64>()
        .map_err(|_| StoreError::CorruptHistory("sequence exceeds u64"))?;
    if parsed == 0 {
        return Err(StoreError::CorruptHistory("sequence is zero"));
    }
    Ok(parsed)
}
