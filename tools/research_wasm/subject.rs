#[inline(never)]
pub fn executed(value: u32) -> u32 {
    value + 1
}

#[inline(never)]
pub fn uncalled(value: u32) -> u32 {
    value + 2
}

#[inline(never)]
pub fn runtime_only() -> u32 {
    #[cfg(target_arch = "wasm32")]
    {
        77
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        88
    }
}
