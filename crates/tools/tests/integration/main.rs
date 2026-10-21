//! The crate's tests through its public interface, one module per part of `src`, in one
//! test binary.

#[path = "compare/properties.rs"]
mod compare_properties;

#[path = "compare/tests.rs"]
mod compare;

#[path = "date_time/tests.rs"]
mod date_time;

#[path = "hex/tests.rs"]
mod hex;

#[path = "html/tests.rs"]
mod html;

#[path = "json/tests.rs"]
mod json;

#[path = "json_tree/tests.rs"]
mod json_tree;

#[path = "print/tests.rs"]
mod print;

#[path = "spell/tests.rs"]
mod spell;

#[path = "tests.rs"]
mod lib_tests;

#[path = "xml/tests.rs"]
mod xml;

#[path = "xml_schema/tests.rs"]
mod xml_schema;

#[path = "xpath/tests.rs"]
mod xpath;
