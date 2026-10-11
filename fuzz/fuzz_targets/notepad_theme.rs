#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    birchpad_fuzz::notepad_theme(data);
});
