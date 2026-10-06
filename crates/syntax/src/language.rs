//! The languages Birchpad knows, and how a file's language is recognized.

use std::path::Path;

/// A language with a tree-sitter grammar.
#[derive(Debug)]
pub struct Language {
    /// Stable id, used in settings, sessions and `-l<id>` on the command line. Where Notepad++
    /// has a `-l` name for the language, the id is that name.
    pub id: &'static str,
    /// Shown in the Language menu and the status bar.
    pub name: &'static str,
    /// The status bar's longer description, as in Notepad++ ("Rust source file").
    pub description: &'static str,
    /// File extensions without the dot, lowercase.
    pub extensions: &'static [&'static str],
    /// Whole file names, matched case-sensitively (`Makefile`).
    pub file_names: &'static [&'static str],
    /// Interpreters named on a `#!` first line (`python3`, `bash`).
    pub interpreters: &'static [&'static str],
    /// Prefixes of the first line that identify the language (`<?xml`).
    pub first_line: &'static [&'static str],
    /// Other names a fenced code block or a heredoc may use for it (`js`, `c++`, `shell`).
    /// Ids, names and extensions need no alias.
    pub aliases: &'static [&'static str],
    pub line_comment: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    pub(crate) grammar: fn() -> tree_sitter::Language,
    /// Highlight queries, concatenated in order; later patterns win over earlier ones.
    pub(crate) highlights: &'static [&'static str],
    /// Where other languages are embedded (scripts in HTML, code blocks in Markdown):
    /// injection queries, concatenated in order.
    pub(crate) injections: &'static [&'static str],
}

impl PartialEq for Language {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Eq for Language {}

macro_rules! grammar {
    ($language:expr) => {{
        fn grammar() -> tree_sitter::Language {
            $language.into()
        }
        grammar
    }};
}

const C_LIKE: (Option<&str>, Option<(&str, &str)>) = (Some("//"), Some(("/*", "*/")));
const HASH: (Option<&str>, Option<(&str, &str)>) = (Some("#"), None);

const fn lang(
    id: &'static str,
    name: &'static str,
    description: &'static str,
    extensions: &'static [&'static str],
    comments: (Option<&'static str>, Option<(&'static str, &'static str)>),
    grammar: fn() -> tree_sitter::Language,
    highlights: &'static [&'static str],
) -> Language {
    Language {
        id,
        name,
        description,
        extensions,
        file_names: &[],
        interpreters: &[],
        first_line: &[],
        aliases: &[],
        line_comment: comments.0,
        block_comment: comments.1,
        grammar,
        highlights,
        injections: &[],
    }
}

/// Every language, sorted by name for the Language menu.
pub static LANGUAGES: &[Language] = &[
    Language {
        aliases: &["assembly", "nasm", "gas"],
        ..lang(
            "asm",
            "Assembly",
            "Assembly language source file",
            &["asm", "s", "nasm"],
            (Some(";"), None),
            grammar!(tree_sitter_asm::LANGUAGE),
            &[tree_sitter_asm::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        aliases: &["shell", "sh", "zsh", "console"],
        file_names: &[
            ".bashrc",
            ".bash_profile",
            ".bash_logout",
            ".profile",
            ".zshrc",
            "PKGBUILD",
        ],
        interpreters: &["bash", "sh", "zsh", "dash", "ksh"],
        ..lang(
            "bash",
            "Bash",
            "Unix script file",
            &["sh", "bash", "zsh", "ksh", "command"],
            HASH,
            grammar!(tree_sitter_bash::LANGUAGE),
            &[tree_sitter_bash::HIGHLIGHT_QUERY],
        )
    },
    Language {
        aliases: &["dosbatch", "winbatch"],
        ..lang(
            "batch",
            "Batch",
            "Batch file",
            &["bat", "cmd", "nt"],
            (Some("REM"), None),
            grammar!(tree_sitter_batch::LANGUAGE),
            &[tree_sitter_batch::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "c",
        "C",
        "C source file",
        &["c", "lex"],
        C_LIKE,
        grammar!(tree_sitter_c::LANGUAGE),
        &[tree_sitter_c::HIGHLIGHT_QUERY],
    ),
    Language {
        aliases: &["csharp", "c#"],
        ..lang(
            "cs",
            "C#",
            "C# source file",
            &["cs", "csx"],
            C_LIKE,
            grammar!(tree_sitter_c_sharp::LANGUAGE),
            &[tree_sitter_c_sharp::HIGHLIGHTS_QUERY],
        )
    },
    // Like Notepad++, `.h` is C++: the C++ grammar parses C headers too.
    Language {
        aliases: &["c++"],
        ..lang(
            "cpp",
            "C++",
            "C++ source file",
            &[
                "cpp", "cxx", "cc", "c++", "h", "hh", "hpp", "hxx", "h++", "ino", "ipp", "tpp",
                "inl",
            ],
            C_LIKE,
            grammar!(tree_sitter_cpp::LANGUAGE),
            &[
                tree_sitter_c::HIGHLIGHT_QUERY,
                tree_sitter_cpp::HIGHLIGHT_QUERY,
            ],
        )
    },
    Language {
        file_names: &["CMakeLists.txt"],
        ..lang(
            "cmake",
            "CMake",
            "CMake file",
            &["cmake"],
            (Some("#"), Some(("#[[", "]]"))),
            grammar!(tree_sitter_cmake::LANGUAGE),
            &[tree_sitter_cmake::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "css",
        "CSS",
        "Cascade Style Sheets File",
        &["css"],
        (None, Some(("/*", "*/"))),
        grammar!(tree_sitter_css::LANGUAGE),
        &[tree_sitter_css::HIGHLIGHTS_QUERY],
    ),
    lang(
        "dart",
        "Dart",
        "Dart source file",
        &["dart"],
        C_LIKE,
        grammar!(tree_sitter_dart::LANGUAGE),
        &[tree_sitter_dart::HIGHLIGHTS_QUERY],
    ),
    Language {
        aliases: &["udiff"],
        ..lang(
            "diff",
            "Diff",
            "Diff file",
            &["diff", "patch"],
            (None, None),
            grammar!(tree_sitter_diff::LANGUAGE),
            &[
                tree_sitter_diff::HIGHLIGHTS_QUERY,
                include_str!("../queries/diff.scm"),
            ],
        )
    },
    Language {
        file_names: &["Dockerfile", "Containerfile", "dockerfile"],
        aliases: &["docker"],
        ..lang(
            "dockerfile",
            "Dockerfile",
            "Dockerfile",
            &["dockerfile", "containerfile"],
            HASH,
            grammar!(tree_sitter_containerfile::LANGUAGE),
            &[tree_sitter_containerfile::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &["mix.lock"],
        interpreters: &["elixir"],
        ..lang(
            "elixir",
            "Elixir",
            "Elixir source file",
            &["ex", "exs"],
            HASH,
            grammar!(tree_sitter_elixir::LANGUAGE),
            &[tree_sitter_elixir::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &["rebar.config"],
        interpreters: &["escript"],
        ..lang(
            "erlang",
            "Erlang",
            "Erlang source file",
            &["erl", "hrl", "escript"],
            (Some("%"), None),
            grammar!(tree_sitter_erlang::LANGUAGE),
            &[tree_sitter_erlang::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        aliases: &["golang"],
        ..lang(
            "go",
            "Go",
            "Go source file",
            &["go"],
            C_LIKE,
            grammar!(tree_sitter_go::LANGUAGE),
            &[tree_sitter_go::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "graphql",
        "GraphQL",
        "GraphQL file",
        &["graphql", "graphqls", "gql"],
        HASH,
        grammar!(tree_sitter_graphql::LANGUAGE),
        &[include_str!("../queries/graphql.scm")],
    ),
    Language {
        file_names: &["Jenkinsfile"],
        interpreters: &["groovy"],
        aliases: &["gradle"],
        ..lang(
            "groovy",
            "Groovy",
            "Groovy source file",
            &["groovy", "gradle", "gvy", "gy", "gsh"],
            C_LIKE,
            grammar!(tree_sitter_groovy::LANGUAGE),
            &[include_str!("../queries/groovy.scm")],
        )
    },
    Language {
        interpreters: &["runhaskell", "runghc"],
        ..lang(
            "haskell",
            "Haskell",
            "Haskell source file",
            &["hs", "hsc"],
            (Some("--"), Some(("{-", "-}"))),
            grammar!(tree_sitter_haskell::LANGUAGE),
            &[tree_sitter_haskell::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        aliases: &["terraform"],
        ..lang(
            "hcl",
            "HCL",
            "HashiCorp Configuration Language file",
            &["hcl", "tf", "tfvars", "nomad"],
            (Some("#"), Some(("/*", "*/"))),
            grammar!(tree_sitter_hcl::LANGUAGE),
            &[include_str!("../queries/hcl.scm")],
        )
    },
    Language {
        injections: &[include_str!("../queries/html-injections.scm")],
        aliases: &["xhtml"],
        first_line: &["<!DOCTYPE html", "<!doctype html", "<html", "<HTML"],
        ..lang(
            "html",
            "HTML",
            "Hyper Text Markup Language file",
            &["html", "htm", "shtml", "shtm", "xhtml", "xht", "hta"],
            (None, Some(("<!--", "-->"))),
            grammar!(tree_sitter_html::LANGUAGE),
            &[tree_sitter_html::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &[".editorconfig", ".gitconfig"],
        ..lang(
            "ini",
            "INI",
            "MS ini file",
            &["ini", "inf", "url", "wer", "cfg", "desktop"],
            (Some(";"), None),
            grammar!(tree_sitter_ini::LANGUAGE),
            &[tree_sitter_ini::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "java",
        "Java",
        "Java source file",
        &["java"],
        C_LIKE,
        grammar!(tree_sitter_java::LANGUAGE),
        &[tree_sitter_java::HIGHLIGHTS_QUERY],
    ),
    Language {
        injections: &[include_str!("../queries/javascript-injections.scm")],
        aliases: &["js", "node", "jsx"],
        interpreters: &["node", "nodejs", "deno"],
        ..lang(
            "javascript",
            "JavaScript",
            "JavaScript file",
            &["js", "mjs", "cjs", "jsx", "jsm"],
            C_LIKE,
            grammar!(tree_sitter_javascript::LANGUAGE),
            &[
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
            ],
        )
    },
    lang(
        "json",
        "JSON",
        "JSON file",
        &[
            "json",
            "jsonc",
            "json5",
            "geojson",
            "webmanifest",
            "babelrc",
            "eslintrc",
        ],
        C_LIKE,
        grammar!(tree_sitter_json::LANGUAGE),
        &[include_str!("../queries/json.scm")],
    ),
    Language {
        interpreters: &["kotlin"],
        ..lang(
            "kotlin",
            "Kotlin",
            "Kotlin source file",
            &["kt", "kts"],
            C_LIKE,
            grammar!(tree_sitter_kotlin_sg::LANGUAGE),
            &[tree_sitter_kotlin_sg::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        interpreters: &["lua", "luajit"],
        ..lang(
            "lua",
            "Lua",
            "Lua source File",
            &["lua"],
            (Some("--"), Some(("--[[", "]]"))),
            grammar!(tree_sitter_lua::LANGUAGE),
            &[tree_sitter_lua::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        aliases: &["make"],
        file_names: &["Makefile", "makefile", "GNUmakefile"],
        ..lang(
            "makefile",
            "Makefile",
            "Makefile",
            &["mak", "mk"],
            HASH,
            grammar!(tree_sitter_make::LANGUAGE),
            &[tree_sitter_make::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        injections: &[tree_sitter_md::INJECTION_QUERY_BLOCK],
        aliases: &["md"],
        ..lang(
            "markdown",
            "Markdown",
            "Markdown file",
            &["md", "markdown", "mdown", "mkd", "mkdn"],
            (None, Some(("<!--", "-->"))),
            grammar!(tree_sitter_md::LANGUAGE),
            &[tree_sitter_md::HIGHLIGHT_QUERY_BLOCK],
        )
    },
    Language {
        file_names: &["nginx.conf"],
        ..lang(
            "nginx",
            "nginx",
            "nginx configuration file",
            &["nginx"],
            HASH,
            grammar!(tree_sitter_nginx::LANGUAGE),
            &[include_str!("../queries/nginx.scm")],
        )
    },
    lang(
        "nix",
        "Nix",
        "Nix expression file",
        &["nix"],
        (Some("#"), Some(("/*", "*/"))),
        grammar!(tree_sitter_nix::LANGUAGE),
        &[tree_sitter_nix::HIGHLIGHTS_QUERY],
    ),
    Language {
        aliases: &["objective-c", "objectivec", "obj-c"],
        ..lang(
            "objc",
            "Objective-C",
            "Objective-C source file",
            &["m", "mm"],
            C_LIKE,
            grammar!(tree_sitter_objc::LANGUAGE),
            &[
                tree_sitter_c::HIGHLIGHT_QUERY,
                tree_sitter_objc::HIGHLIGHTS_QUERY,
            ],
        )
    },
    Language {
        aliases: &["delphi", "objectpascal"],
        ..lang(
            "pascal",
            "Pascal",
            "Pascal source file",
            &["pas", "pp", "dpr", "dpk", "lpr"],
            (Some("//"), Some(("{", "}"))),
            grammar!(tree_sitter_pascal::LANGUAGE),
            &[include_str!("../queries/pascal.scm")],
        )
    },
    Language {
        injections: &[include_str!("../queries/php-injections.scm")],
        interpreters: &["php"],
        first_line: &["<?php"],
        ..lang(
            "php",
            "PHP",
            "PHP Hypertext Preprocessor file",
            &["php", "php3", "php4", "php5", "phps", "phpt", "phtml"],
            C_LIKE,
            grammar!(tree_sitter_php::LANGUAGE_PHP),
            &[tree_sitter_php::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        aliases: &["pwsh", "ps"],
        interpreters: &["pwsh", "powershell"],
        ..lang(
            "powershell",
            "PowerShell",
            "Windows PowerShell",
            &["ps1", "psm1", "psd1"],
            (Some("#"), Some(("<#", "#>"))),
            grammar!(tree_sitter_powershell::LANGUAGE),
            &[tree_sitter_powershell::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "properties",
        "Properties",
        "Properties file",
        &["properties"],
        HASH,
        grammar!(tree_sitter_properties::LANGUAGE),
        &[tree_sitter_properties::HIGHLIGHTS_QUERY],
    ),
    Language {
        aliases: &["protobuf"],
        ..lang(
            "proto",
            "Protocol Buffers",
            "Protocol Buffers file",
            &["proto"],
            C_LIKE,
            grammar!(tree_sitter_proto::LANGUAGE),
            &[include_str!("../queries/proto.scm")],
        )
    },
    Language {
        aliases: &["py", "python3", "py3"],
        file_names: &["SConstruct", "SConscript", "BUCK", "Snakefile"],
        interpreters: &["python", "python2", "python3", "pypy", "pypy3"],
        ..lang(
            "python",
            "Python",
            "Python file",
            &["py", "pyw", "pyi", "pyx", "pxd", "gyp", "bzl", "wsgi"],
            HASH,
            grammar!(tree_sitter_python::LANGUAGE),
            &[tree_sitter_python::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &[".Rprofile"],
        interpreters: &["Rscript"],
        ..lang(
            "r",
            "R",
            "R programming language",
            &["r"],
            HASH,
            grammar!(tree_sitter_r::LANGUAGE),
            &[tree_sitter_r::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &[
            "Gemfile",
            "Rakefile",
            "Guardfile",
            "Vagrantfile",
            "Podfile",
            "Brewfile",
        ],
        interpreters: &["ruby", "jruby"],
        ..lang(
            "ruby",
            "Ruby",
            "Ruby file",
            &["rb", "rbw", "rake", "gemspec", "ru", "erb"],
            (Some("#"), Some(("=begin", "=end"))),
            grammar!(tree_sitter_ruby::LANGUAGE),
            &[tree_sitter_ruby::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "rust",
        "Rust",
        "Rust file",
        &["rs"],
        C_LIKE,
        grammar!(tree_sitter_rust::LANGUAGE),
        &[tree_sitter_rust::HIGHLIGHTS_QUERY],
    ),
    Language {
        interpreters: &["scala"],
        ..lang(
            "scala",
            "Scala",
            "Scala source file",
            &["scala", "sc", "sbt"],
            C_LIKE,
            grammar!(tree_sitter_scala::LANGUAGE),
            &[tree_sitter_scala::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "solidity",
        "Solidity",
        "Solidity source file",
        &["sol"],
        C_LIKE,
        grammar!(tree_sitter_solidity::LANGUAGE),
        &[include_str!("../queries/solidity.scm")],
    ),
    lang(
        "sql",
        "SQL",
        "Structured Query Language file",
        &["sql", "ddl", "dml"],
        (Some("--"), Some(("/*", "*/"))),
        grammar!(tree_sitter_sequel::LANGUAGE),
        &[tree_sitter_sequel::HIGHLIGHTS_QUERY],
    ),
    Language {
        injections: &[include_str!("../queries/svelte-injections.scm")],
        ..lang(
            "svelte",
            "Svelte",
            "Svelte component",
            &["svelte"],
            (None, Some(("<!--", "-->"))),
            grammar!(tree_sitter_svelte_ng::LANGUAGE),
            &[
                tree_sitter_html::HIGHLIGHTS_QUERY,
                tree_sitter_svelte_ng::HIGHLIGHTS_QUERY,
            ],
        )
    },
    Language {
        interpreters: &["swift"],
        ..lang(
            "swift",
            "Swift",
            "Swift source file",
            &["swift"],
            C_LIKE,
            grammar!(tree_sitter_swift::LANGUAGE),
            &[tree_sitter_swift::HIGHLIGHTS_QUERY],
        )
    },
    Language {
        file_names: &["Cargo.lock", "Pipfile", "poetry.lock", "uv.lock"],
        ..lang(
            "toml",
            "TOML",
            "Tom's Obvious Minimal Language file",
            &["toml"],
            HASH,
            grammar!(tree_sitter_toml_ng::LANGUAGE),
            &[include_str!("../queries/toml.scm")],
        )
    },
    Language {
        injections: &[include_str!("../queries/javascript-injections.scm")],
        aliases: &["ts"],
        ..lang(
            "typescript",
            "TypeScript",
            "TypeScript file",
            &["ts", "mts", "cts"],
            C_LIKE,
            grammar!(tree_sitter_typescript::LANGUAGE_TYPESCRIPT),
            &[
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
        )
    },
    Language {
        injections: &[include_str!("../queries/javascript-injections.scm")],
        ..lang(
            "tsx",
            "TypeScript JSX",
            "TypeScript JSX file",
            &["tsx"],
            C_LIKE,
            grammar!(tree_sitter_typescript::LANGUAGE_TSX),
            &[
                tree_sitter_javascript::HIGHLIGHT_QUERY,
                tree_sitter_javascript::JSX_HIGHLIGHT_QUERY,
                tree_sitter_typescript::HIGHLIGHTS_QUERY,
            ],
        )
    },
    Language {
        aliases: &["svg"],
        first_line: &["<?xml"],
        ..lang(
            "xml",
            "XML",
            "eXtensible Markup Language file",
            &[
                "xml", "xaml", "xsl", "xslt", "xsd", "xul", "kml", "svg", "mxml", "xsml", "wsdl",
                "xlf", "xliff", "xbl", "sxbl", "sitemap", "gml", "gpx", "plist", "vcproj",
                "vcxproj", "csproj", "vbproj", "fsproj", "props", "targets", "nuspec", "resx",
                "manifest", "config", "rss", "atom",
            ],
            (None, Some(("<!--", "-->"))),
            grammar!(tree_sitter_xml::LANGUAGE_XML),
            &[tree_sitter_xml::XML_HIGHLIGHT_QUERY],
        )
    },
    Language {
        aliases: &["yml"],
        ..lang(
            "yaml",
            "YAML",
            "YAML Ain't Markup Language",
            &["yml", "yaml"],
            HASH,
            grammar!(tree_sitter_yaml::LANGUAGE),
            &[tree_sitter_yaml::HIGHLIGHTS_QUERY],
        )
    },
    lang(
        "zig",
        "Zig",
        "Zig source file",
        &["zig", "zon"],
        (Some("//"), None),
        grammar!(tree_sitter_zig::LANGUAGE),
        &[tree_sitter_zig::HIGHLIGHTS_QUERY],
    ),
];

/// Languages found only inside others, which the Language menu does not list: Markdown's inline
/// syntax, regular expressions in JavaScript, JSDoc comments.
pub(crate) static EMBEDDED: &[Language] = &[
    Language {
        injections: &[tree_sitter_md::INJECTION_QUERY_INLINE],
        ..lang(
            "markdown_inline",
            "Markdown (inline)",
            "Markdown inline syntax",
            &[],
            (None, None),
            grammar!(tree_sitter_md::INLINE_LANGUAGE),
            &[tree_sitter_md::HIGHLIGHT_QUERY_INLINE],
        )
    },
    lang(
        "regex",
        "Regular expression",
        "Regular expression",
        &[],
        (None, None),
        grammar!(tree_sitter_regex::LANGUAGE),
        &[tree_sitter_regex::HIGHLIGHTS_QUERY],
    ),
    lang(
        "jsdoc",
        "JSDoc",
        "JSDoc comment",
        &[],
        (None, None),
        grammar!(tree_sitter_jsdoc::LANGUAGE),
        &[tree_sitter_jsdoc::HIGHLIGHTS_QUERY],
    ),
];

/// Every language with a grammar: those of the Language menu, then the embedded ones.
pub(crate) fn all() -> impl Iterator<Item = &'static Language> {
    LANGUAGES.iter().chain(EMBEDDED)
}

/// The language with `id`, if Birchpad knows it.
pub fn by_id(id: &str) -> Option<&'static Language> {
    let id = id.to_ascii_lowercase();
    LANGUAGES.iter().find(|language| language.id == id)
}

/// The language an embedded block names: a fenced code block's info string (`rust`, `js`,
/// `C++`, `{.python}`), a tagged template (`css`), a heredoc delimiter (`SQL`), or an injection
/// query. Tried as an id, an alias, a name and a file extension, ignoring case.
pub fn injected(name: &str) -> Option<&'static Language> {
    let name = name
        .trim()
        .trim_start_matches(['{', '.'])
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | '}' | '='))
        .next()?
        .to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    all()
        .find(|language| language.id == name)
        .or_else(|| all().find(|language| language.aliases.contains(&name.as_str())))
        .or_else(|| {
            LANGUAGES
                .iter()
                .find(|language| language.name.eq_ignore_ascii_case(&name))
        })
        .or_else(|| {
            LANGUAGES
                .iter()
                .find(|language| language.extensions.contains(&name.as_str()))
        })
}

/// Recognizes a file's language from its name, then from its first line (a `#!` interpreter
/// or a signature like `<?xml`). Returns `None` for plain text.
pub fn detect(path: Option<&Path>, first_line: &str) -> Option<&'static Language> {
    let by_name = path
        .and_then(|path| path.file_name())
        .and_then(|name| name.to_str())
        .and_then(|name| {
            LANGUAGES
                .iter()
                .find(|language| language.file_names.contains(&name))
        });
    let by_extension = || {
        let extension = path?.extension()?.to_str()?.to_ascii_lowercase();
        LANGUAGES
            .iter()
            .find(|language| language.extensions.contains(&extension.as_str()))
    };
    by_name
        .or_else(by_extension)
        .or_else(|| detect_first_line(first_line))
}

fn detect_first_line(line: &str) -> Option<&'static Language> {
    let line = line.trim_start_matches('\u{feff}');
    if let Some(command) = line.strip_prefix("#!") {
        // `#!/usr/bin/env python3 -u` or `#!/bin/bash`.
        let mut words = command.split_whitespace();
        let mut program = words.next()?;
        if program.ends_with("/env") {
            program = words.find(|word| !word.starts_with('-'))?;
        }
        let interpreter = program.rsplit('/').next()?;
        return LANGUAGES
            .iter()
            .find(|language| language.interpreters.contains(&interpreter));
    }
    LANGUAGES.iter().find(|language| {
        language
            .first_line
            .iter()
            .any(|signature| line.starts_with(signature))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn detected(path: &str, first_line: &str) -> Option<&'static str> {
        detect(Some(Path::new(path)), first_line).map(|language| language.id)
    }

    #[test]
    fn languages_are_sorted_and_unique() {
        let names: Vec<&str> = LANGUAGES.iter().map(|language| language.name).collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(names, sorted, "LANGUAGES must stay sorted by name");
        for (i, language) in LANGUAGES.iter().enumerate() {
            assert!(
                LANGUAGES[i + 1..]
                    .iter()
                    .all(|other| other.id != language.id),
                "duplicate id {}",
                language.id
            );
            for extension in language.extensions {
                assert_eq!(*extension, extension.to_ascii_lowercase());
                let owners: Vec<&str> = LANGUAGES
                    .iter()
                    .filter(|other| other.extensions.contains(extension))
                    .map(|other| other.id)
                    .collect();
                assert_eq!(owners.len(), 1, ".{extension} belongs to {owners:?}");
            }
        }
    }

    #[test]
    fn detects_by_name_extension_and_first_line() {
        assert_eq!(detected("src/main.RS", ""), Some("rust"));
        assert_eq!(detected("include/api.h", ""), Some("cpp"));
        assert_eq!(detected("Makefile", ""), Some("makefile"));
        assert_eq!(detected("Cargo.lock", ""), Some("toml"));
        assert_eq!(detected("notes.txt", ""), None);
        assert_eq!(
            detected("run", "#!/usr/bin/env -S python3 -u"),
            Some("python")
        );
        assert_eq!(detected("run", "#!/bin/bash"), Some("bash"));
        assert_eq!(
            detected("data", "\u{feff}<?xml version=\"1.0\"?>"),
            Some("xml")
        );
        assert_eq!(detected("page", "<!DOCTYPE html>"), Some("html"));
        assert_eq!(detected("index", "<?php echo 1;"), Some("php"));
        // The name wins over the first line.
        assert_eq!(detected("build.py", "#!/bin/sh"), Some("python"));
        assert_eq!(by_id("CPP").map(|language| language.name), Some("C++"));
        assert_eq!(detected("app/build.gradle.kts", ""), Some("kotlin"));
        assert_eq!(detected("build.gradle", ""), Some("groovy"));
        assert_eq!(detected("Dockerfile", ""), Some("dockerfile"));
        assert_eq!(detected("CMakeLists.txt", ""), Some("cmake"));
        assert_eq!(detected("main.tf", ""), Some("hcl"));
        // Embedded languages are not files' languages.
        assert_eq!(by_id("markdown_inline"), None);
    }

    #[test]
    fn embedded_blocks_name_languages_many_ways() {
        let named = |name| injected(name).map(|language| language.id);
        assert_eq!(named("rust"), Some("rust"));
        assert_eq!(named("JS"), Some("javascript"));
        assert_eq!(named("c++"), Some("cpp"));
        assert_eq!(named("C#"), Some("cs"));
        assert_eq!(named("shell"), Some("bash"));
        assert_eq!(named("{.python}"), Some("python"));
        assert_eq!(named("rust,ignore"), Some("rust"));
        assert_eq!(named("kt"), Some("kotlin"));
        assert_eq!(named("Objective-C"), Some("objc"));
        assert_eq!(named("SQL"), Some("sql"));
        assert_eq!(named("markdown_inline"), Some("markdown_inline"));
        assert_eq!(named("div"), None);
        assert_eq!(named(""), None);
    }
}
