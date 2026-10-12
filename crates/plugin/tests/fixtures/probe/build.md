# Building the probe plugin

```bash
cd crates/plugin/tests/fixtures/probe
cargo build --release --target wasm32-unknown-unknown
wasm-tools component new target/wasm32-unknown-unknown/release/probe.wasm -o ../probe.wasm
```

`wasm-tools` comes from `cargo install wasm-tools`. The built component is committed, so the
tests need neither.
