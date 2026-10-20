use std::cell::RefCell;
use std::path::Path;

/// What tests let the system say, and what Birchpad asked of it.
#[derive(Debug, Default)]
pub struct Fake {
    pub unsupported: bool,
    pub default: bool,
    pub refuse: bool,
    pub registered: usize,
    pub opened: Vec<String>,
}

thread_local! {
    pub static FAKE: RefCell<Fake> = RefCell::default();
}

pub(super) fn supported() -> bool {
    FAKE.with(|fake| !fake.borrow().unsupported)
}

pub(super) fn is_default(_: &str) -> bool {
    FAKE.with(|fake| fake.borrow().default)
}

pub(super) fn register(_: &Path) -> std::io::Result<()> {
    FAKE.with(|fake| {
        let mut fake = fake.borrow_mut();
        if fake.refuse {
            return Err(std::io::Error::other("access denied"));
        }
        fake.registered += 1;
        Ok(())
    })
}

pub(super) fn open(page: &str, _: &mut gpui_kit::App) {
    FAKE.with(|fake| fake.borrow_mut().opened.push(page.to_owned()));
}
