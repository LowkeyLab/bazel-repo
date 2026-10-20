mod subject;

struct Sink(usize);
static mut PROFILE: [u8; 65536] = [0; 65536];
impl minicov::CoverageWriter for Sink {
    fn write(&mut self, data: &[u8]) -> Result<(), minicov::CoverageWriteError> {
        if data.len() > 65536 - self.0 {
            return Err(minicov::CoverageWriteError);
        }
        unsafe {
            core::ptr::copy_nonoverlapping(
                data.as_ptr(),
                (&raw mut PROFILE).cast::<u8>().add(self.0),
                data.len(),
            );
        }
        self.0 += data.len();
        Ok(())
    }
}

#[wasm_bindgen::prelude::wasm_bindgen]
pub extern "C" fn run() -> u32 {
    let _uncalled = subject::uncalled as fn(u32) -> u32;
    subject::executed(5) + subject::runtime_only()
}

#[wasm_bindgen::prelude::wasm_bindgen]
pub extern "C" fn dump_profile() -> usize {
    let mut sink = Sink(0);
    unsafe { minicov::capture_coverage(&mut sink).unwrap() };
    sink.0
}

#[wasm_bindgen::prelude::wasm_bindgen]
pub extern "C" fn profile_pointer() -> *const u8 {
    (&raw const PROFILE).cast::<u8>()
}
