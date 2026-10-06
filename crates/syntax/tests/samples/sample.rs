use std::fmt;

/// A greeter.
#[derive(Debug)]
struct Greeter<'a> {
    name: &'a str,
}

fn main() {
    let count = 42_u8;
    println!("Hello, {}!\n", count);
}
