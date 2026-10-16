#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    birchpad_fuzz::rhai_script(data);
});
