#[path = "nested/value.rs"]
mod value;

/// Return the declared fixture value.
///
/// # Errors
///
/// This fixture always succeeds.
pub const fn answer() -> anyhow::Result<u32> {
    Ok(value::ANSWER)
}
