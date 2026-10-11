//! Notepad++'s auto-completion API files (`autoCompletion/<language>.xml`): the keywords and
//! functions of a language, with the parameters of each function's overloads for call tips.
//!
//! ```xml
//! <NotepadPlus>
//!   <AutoComplete language="C">
//!     <Environment ignoreCase="no" startFunc="(" stopFunc=")" paramSeparator="," />
//!     <KeyWord name="abs" func="yes">
//!       <Overload retVal="int" descr="Absolute value"><Param name="int n" /></Overload>
//!     </KeyWord>
//!     <KeyWord name="auto" />
//!   </AutoComplete>
//! </NotepadPlus>
//! ```

/// An API file that does not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0}")]
pub struct ApiError(String);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiFile {
    pub ignore_case: bool,
    /// What opens and closes a call's parameters, and separates them.
    pub start_func: char,
    pub stop_func: char,
    pub param_separator: char,
    /// Sorted by name.
    pub keywords: Vec<ApiKeyword>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiKeyword {
    pub name: String,
    /// The forms of a function; empty for a plain keyword.
    pub overloads: Vec<ApiOverload>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiOverload {
    pub return_value: String,
    pub description: String,
    pub params: Vec<String>,
}

impl ApiOverload {
    /// `int abs(int n)`.
    pub fn signature(&self, name: &str) -> String {
        let call = format!("{name}({})", self.params.join(", "));
        if self.return_value.is_empty() {
            call
        } else {
            format!("{} {call}", self.return_value)
        }
    }
}

impl ApiFile {
    /// Reads an API file.
    pub fn parse(xml: &str) -> Result<Self, ApiError> {
        let xml = xml.strip_prefix('\u{feff}').unwrap_or(xml);
        let document = roxmltree::Document::parse(xml)
            .map_err(|error| ApiError(format!("not an API file: {error}")))?;
        let complete = document
            .descendants()
            .find(|node| node.has_tag_name("AutoComplete"))
            .ok_or_else(|| ApiError("no <AutoComplete> in the file".into()))?;
        let environment = complete
            .children()
            .find(|node| node.has_tag_name("Environment"));
        let attribute = |name: &str| environment.and_then(|node| node.attribute(name));
        let single = |name: &str, default: char| {
            attribute(name)
                .and_then(|value| {
                    let mut chars = value.chars();
                    let first = chars.next()?;
                    chars.next().is_none().then_some(first)
                })
                .unwrap_or(default)
        };
        let mut keywords: Vec<ApiKeyword> = complete
            .children()
            .filter(|node| node.has_tag_name("KeyWord"))
            .filter_map(|node| {
                let name = node.attribute("name")?.trim();
                if name.is_empty() {
                    return None;
                }
                let overloads = node
                    .children()
                    .filter(|child| child.has_tag_name("Overload"))
                    .map(|overload| ApiOverload {
                        return_value: overload.attribute("retVal").unwrap_or_default().to_owned(),
                        description: overload.attribute("descr").unwrap_or_default().to_owned(),
                        params: overload
                            .children()
                            .filter(|param| param.has_tag_name("Param"))
                            .filter_map(|param| param.attribute("name").map(str::to_owned))
                            .collect(),
                    })
                    .collect();
                Some(ApiKeyword {
                    name: name.to_owned(),
                    overloads,
                })
            })
            .collect();
        keywords.sort_by(|a, b| a.name.cmp(&b.name));
        keywords.dedup_by(|a, b| a.name == b.name);
        Ok(Self {
            ignore_case: attribute("ignoreCase")
                .is_some_and(|value| value.eq_ignore_ascii_case("yes")),
            start_func: single("startFunc", '('),
            stop_func: single("stopFunc", ')'),
            param_separator: single("paramSeparator", ','),
            keywords,
        })
    }

    /// The keywords that start with `prefix` and are longer than it.
    pub fn completions(&self, prefix: &str) -> Vec<&ApiKeyword> {
        let lower = prefix.to_lowercase();
        self.keywords
            .iter()
            .filter(|keyword| {
                keyword.name.len() > prefix.len()
                    && if self.ignore_case {
                        keyword.name.to_lowercase().starts_with(&lower)
                    } else {
                        keyword.name.starts_with(prefix)
                    }
            })
            .collect()
    }

    /// The function named `name`, if it has overloads.
    pub fn function(&self, name: &str) -> Option<&ApiKeyword> {
        self.keywords
            .iter()
            .find(|keyword| {
                if self.ignore_case {
                    keyword.name.eq_ignore_ascii_case(name)
                } else {
                    keyword.name == name
                }
            })
            .filter(|keyword| !keyword.overloads.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const C: &str = r#"<?xml version="1.0" encoding="UTF-8" ?>
<NotepadPlus>
    <AutoComplete language="C">
        <Environment ignoreCase="no" startFunc="(" stopFunc=")" paramSeparator="," terminal=";" />
        <KeyWord name="auto" />
        <KeyWord name="abs" func="yes">
            <Overload retVal="int" descr="Absolute value">
                <Param name="int n" />
            </Overload>
        </KeyWord>
        <KeyWord name="atan2" func="yes">
            <Overload retVal="double"><Param name="double y" /><Param name="double x" /></Overload>
            <Overload retVal="float"><Param name="float y" /><Param name="float x" /></Overload>
        </KeyWord>
        <KeyWord name="" />
        <KeyWord />
        <KeyWord name="abs" />
    </AutoComplete>
</NotepadPlus>"#;

    #[test]
    fn reads_keywords_and_overloads() {
        let api = ApiFile::parse(C).unwrap();
        let names: Vec<_> = api
            .keywords
            .iter()
            .map(|keyword| keyword.name.as_str())
            .collect();
        assert_eq!(names, ["abs", "atan2", "auto"]);
        assert!(!api.ignore_case);
        assert_eq!(
            (api.start_func, api.stop_func, api.param_separator),
            ('(', ')', ',')
        );
        let atan2 = api.function("atan2").unwrap();
        assert_eq!(atan2.overloads.len(), 2);
        assert_eq!(
            atan2.overloads[0].signature("atan2"),
            "double atan2(double y, double x)"
        );
        assert_eq!(
            api.function("abs").unwrap().overloads[0].description,
            "Absolute value"
        );
        // A keyword is not a function; case matters here.
        assert!(api.function("auto").is_none());
        assert!(api.function("ABS").is_none());
        let found: Vec<_> = api
            .completions("a")
            .iter()
            .map(|keyword| keyword.name.as_str())
            .collect();
        assert_eq!(found, ["abs", "atan2", "auto"]);
        assert!(api.completions("abs").is_empty());
        assert!(api.completions("A").is_empty());
    }

    #[test]
    fn options_and_defaults() {
        let api = ApiFile::parse(
            "\u{feff}<NotepadPlus><AutoComplete><Environment ignoreCase=\"yes\" startFunc=\"[\" stopFunc=\"]]\" /><KeyWord name=\"Print\" func=\"yes\"><Overload /></KeyWord></AutoComplete></NotepadPlus>",
        )
        .unwrap();
        assert!(api.ignore_case);
        assert_eq!(api.start_func, '[');
        // Not one character: the default.
        assert_eq!(api.stop_func, ')');
        assert_eq!(api.completions("pr").len(), 1);
        assert_eq!(
            api.function("PRINT").unwrap().overloads[0].signature("Print"),
            "Print()"
        );
        let empty = ApiFile::parse("<NotepadPlus><AutoComplete/></NotepadPlus>").unwrap();
        assert!(empty.keywords.is_empty());
    }

    #[test]
    fn refuses_other_files() {
        for xml in ["", "<x", "<NotepadPlus/>", "<Session><File/></Session>"] {
            assert!(ApiFile::parse(xml).is_err(), "{xml}");
        }
    }
}
