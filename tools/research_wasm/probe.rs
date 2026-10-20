#![no_std]

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn executed(value: u32) -> u32 {
    value + 1
}

#[unsafe(no_mangle)]
pub extern "C" fn uncalled(value: u32) -> u32 {
    value + 2
}
