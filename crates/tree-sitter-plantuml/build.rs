//! Compiles the parser `tree-sitter generate` wrote from `grammar.js`, and its scanner.

fn main() {
    let src = std::path::Path::new("src");
    cc::Build::new()
        .std("c11")
        .include(src)
        .file(src.join("parser.c"))
        .file(src.join("scanner.c"))
        .warnings(false)
        .compile("tree-sitter-plantuml");
    println!("cargo:rerun-if-changed=src/parser.c");
    println!("cargo:rerun-if-changed=src/scanner.c");
}
