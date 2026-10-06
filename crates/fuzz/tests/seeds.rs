//! The fuzz targets over their seeds in `fuzz/seeds`, and over damaged copies of them: a small,
//! repeatable fuzzer that `cargo test` runs, while libFuzzer runs the same targets longer in CI.
//!
//! Seeds named `reject-*` must be refused, the others accepted. Each seed is also cut at every
//! length and mutated a few hundred times (`BIRCHPAD_FUZZ_MUTANTS` sets how many): bits
//! flipped, bytes changed, ranges dropped and repeated, tokens and pieces of other seeds put
//! in. An input that makes a target panic is saved in `target/fuzz-failures` and named in the
//! failure; once the bug is fixed, it becomes a seed.

use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use birchpad_fuzz::TARGETS;

const MUTANTS: usize = 300;

fn fuzz_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fuzz")
}

/// The seeds of `target`, by file name.
fn seeds(target: &str) -> Vec<(String, Vec<u8>)> {
    let dir = fuzz_dir().join("seeds").join(target);
    let mut seeds: Vec<_> = std::fs::read_dir(&dir)
        .unwrap_or_else(|error| panic!("{}: {error}", dir.display()))
        .map(|entry| {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_string_lossy().into_owned();
            (name, std::fs::read(&path).unwrap())
        })
        .collect();
    seeds.sort();
    seeds
}

/// xorshift64: the same mutants on every run.
struct Rng(u64);

impl Rng {
    fn new(name: &str) -> Self {
        let seed = name.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    /// A number in `0..n`, `n > 0`.
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

/// Pieces of the formats the targets read, and bytes that break encodings.
const TOKENS: &[&[u8]] = &[
    b"\0",
    b"\n",
    b"\r\n",
    b"\r",
    b"\"",
    b"'",
    b"'''",
    b"\\",
    b"\\u0000",
    b"<",
    b">",
    b"</",
    b"/>",
    b"&amp;",
    b"&#0;",
    b"&#x110000;",
    b"<![CDATA[",
    b"]]>",
    b"<!--",
    b"=",
    b"[",
    b"]",
    b"[[",
    b"{",
    b"}",
    b",",
    b":",
    b".",
    b"-",
    b"--",
    b"0",
    b"-1",
    b"18446744073709551616",
    b"1e400",
    b"nan",
    b"inf",
    b"true",
    b"null",
    b"\xef\xbb\xbf",
    b"\xff\xfe",
    b"\xfe\xff",
    b"\xc3",
    b"\xed\xa0\x80",
    b"\xf0\x9f\x98\x80",
    b"\xd8\x3d",
];

/// `input` with one to four random changes.
fn mutate(input: &[u8], others: &[Vec<u8>], rng: &mut Rng) -> Vec<u8> {
    let mut data = input.to_vec();
    for _ in 0..=rng.below(4) {
        let len = data.len();
        match rng.below(6) {
            0 if len > 0 => {
                let at = rng.below(len);
                data[at] ^= 1 << rng.below(8);
            }
            1 if len > 0 => {
                let at = rng.below(len);
                data[at] = [0, b'\t', b' ', 0x7f, 0x80, 0xff][rng.below(6)];
            }
            2 if len > 0 => {
                let start = rng.below(len);
                let end = start + rng.below(len - start + 1);
                data.drain(start..end);
            }
            3 if len > 0 => {
                let start = rng.below(len);
                let end = (start + 1 + rng.below(32)).min(len);
                let piece = data[start..end].to_vec();
                let at = rng.below(len + 1);
                data.splice(at..at, piece);
            }
            4 => {
                let token = TOKENS[rng.below(TOKENS.len())];
                let at = rng.below(len + 1);
                data.splice(at..at, token.iter().copied());
            }
            _ => {
                let other = &others[rng.below(others.len())];
                if !other.is_empty() {
                    let start = rng.below(other.len());
                    let end = (start + 1 + rng.below(64)).min(other.len());
                    let at = rng.below(len + 1);
                    data.splice(at..at, other[start..end].iter().copied());
                }
            }
        }
    }
    data
}

/// Runs `check` on inputs, keeping the panics.
struct Runner {
    target: &'static str,
    check: fn(&[u8]) -> bool,
    failures: Vec<String>,
}

impl Runner {
    fn run(&mut self, input: &[u8], what: impl FnOnce() -> String) -> Option<bool> {
        match catch_unwind(AssertUnwindSafe(|| (self.check)(input))) {
            Ok(accepted) => Some(accepted),
            Err(panic) => {
                let message = panic
                    .downcast_ref::<String>()
                    .map(String::as_str)
                    .or_else(|| panic.downcast_ref::<&str>().copied())
                    .unwrap_or("?");
                let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fuzz-failures");
                std::fs::create_dir_all(&dir).unwrap();
                let path = dir.join(format!("{}-{}", self.target, self.failures.len()));
                std::fs::write(&path, input).unwrap();
                self.failures.push(format!(
                    "{}: {message}\n  input: {}\n  saved in {}",
                    what(),
                    input.escape_ascii(),
                    path.display()
                ));
                None
            }
        }
    }
}

fn mutants() -> usize {
    std::env::var("BIRCHPAD_FUZZ_MUTANTS")
        .ok()
        .and_then(|count| count.parse().ok())
        .unwrap_or(MUTANTS)
}

/// The target over its seeds, every cut of them, and mutants of them.
fn fuzz(target: &'static str) {
    let check = TARGETS
        .iter()
        .find(|(name, _)| *name == target)
        .unwrap_or_else(|| panic!("no target {target}"))
        .1;
    let seeds = seeds(target);
    assert!(
        seeds.iter().any(|(name, _)| !name.starts_with("reject-")),
        "{target} has no seed it accepts"
    );
    assert!(
        seeds.iter().any(|(name, _)| name.starts_with("reject-")),
        "{target} has no seed it refuses"
    );
    let mut runner = Runner {
        target,
        check,
        failures: Vec::new(),
    };
    let mut wrong = Vec::new();

    runner.run(&[], || "no input".to_owned());
    for (name, bytes) in &seeds {
        let expected = !name.starts_with("reject-");
        if let Some(accepted) = runner.run(bytes, || format!("seed {name}"))
            && accepted != expected
        {
            wrong.push(format!(
                "{name}: {}",
                if accepted { "accepted" } else { "refused" }
            ));
        }
        for cut in 0..bytes.len() {
            runner.run(&bytes[..cut], || format!("{name} cut to {cut} bytes"));
        }
    }

    let others: Vec<Vec<u8>> = seeds.iter().map(|(_, bytes)| bytes.clone()).collect();
    let mut rng = Rng::new(target);
    for (name, bytes) in &seeds {
        for index in 0..mutants() {
            let mutant = mutate(bytes, &others, &mut rng);
            runner.run(&mutant, || format!("mutant {index} of {name}"));
        }
    }

    assert!(wrong.is_empty(), "{target}: seeds\n{}", wrong.join("\n"));
    let mut report = String::new();
    for failure in runner.failures.iter().take(10) {
        writeln!(report, "{failure}").unwrap();
    }
    assert!(
        runner.failures.is_empty(),
        "{target} panicked on {} inputs:\n{report}",
        runner.failures.len()
    );
}

#[test]
fn every_target_has_seeds_and_a_libfuzzer_entry() {
    let dir = fuzz_dir();
    let manifest = std::fs::read_to_string(dir.join("Cargo.toml")).unwrap();
    let mut entries: Vec<String> = std::fs::read_dir(dir.join("fuzz_targets"))
        .unwrap()
        .map(|entry| {
            let path = entry.unwrap().path();
            path.file_stem().unwrap().to_string_lossy().into_owned()
        })
        .collect();
    entries.sort();
    let mut names: Vec<String> = TARGETS.iter().map(|(name, _)| (*name).to_owned()).collect();
    names.sort();
    assert_eq!(entries, names, "fuzz/fuzz_targets and TARGETS differ");
    for name in &names {
        assert!(dir.join("seeds").join(name).is_dir(), "no seeds for {name}");
        assert!(
            manifest.contains(&format!("path = \"fuzz_targets/{name}.rs\"")),
            "fuzz/Cargo.toml has no [[bin]] for {name}"
        );
        let entry = std::fs::read_to_string(dir.join(format!("fuzz_targets/{name}.rs"))).unwrap();
        assert!(
            entry.contains(&format!("birchpad_fuzz::{name}(data)")),
            "fuzz_targets/{name}.rs does not run {name}"
        );
    }
}

#[test]
fn mutants_differ_from_their_seed_and_stay_the_same_from_run_to_run() {
    let seed = b"version = 1\n[main-view]\nactive = 0\n".to_vec();
    let others = vec![seed.clone()];
    let run = || {
        let mut rng = Rng::new("session");
        (0..50)
            .map(|_| mutate(&seed, &others, &mut rng))
            .collect::<Vec<_>>()
    };
    let first = run();
    assert_eq!(first, run());
    assert!(first.iter().filter(|mutant| **mutant != seed).count() > 40);
    // An empty input grows: tokens and pieces of other inputs go in.
    let mut rng = Rng::new("empty");
    assert!((0..50).any(|_| !mutate(&[], &others, &mut rng).is_empty()));
}

macro_rules! fuzz_tests {
    ($($target:ident),* $(,)?) => {
        $(
            #[test]
            fn $target() {
                fuzz(stringify!($target));
            }
        )*
    };
}

fuzz_tests!(
    open_file,
    settings,
    state,
    session,
    workspace_file,
    keymap,
    command_line,
    instance_message,
    manifest,
);
