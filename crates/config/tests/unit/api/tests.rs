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
