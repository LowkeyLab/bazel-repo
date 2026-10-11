fn main() -> anyhow::Result<()> {
    let value = "42".parse::<u32>()?;
    anyhow::ensure!(value == 42, "fixture arithmetic");
    Ok(())
}
