// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::{scan::Scanner, Parser};
use crate::prim::FinalSep;
use expect_test::Expect;
use std::fmt::Display;

pub(super) fn check<T: Display>(parser: impl Parser<T>, input: &str, expect: &Expect) {
    check_map(parser, input, expect, ToString::to_string);
}

pub(super) fn check_opt<T: Display>(parser: impl Parser<Option<T>>, input: &str, expect: &Expect) {
    check_map(parser, input, expect, |value| match value {
        Some(value) => value.to_string(),
        None => "None".to_string(),
    });
}

pub(super) fn check_vec<T: Display>(parser: impl Parser<Vec<T>>, input: &str, expect: &Expect) {
    check_map(parser, input, expect, |values| {
        values
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(",\n")
    });
}

pub(super) fn check_seq<T: Display>(
    parser: impl Parser<(Vec<T>, FinalSep)>,
    input: &str,
    expect: &Expect,
) {
    check_map(parser, input, expect, |(values, sep)| {
        format!(
            "({}, {sep:?})",
            values
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>()
                .join(",\n")
        )
    });
}

fn check_map<T>(
    mut parser: impl Parser<T>,
    input: &str,
    expect: &Expect,
    f: impl FnOnce(&T) -> String,
) {
    let mut scanner = Scanner::new(input);
    let result = parser(&mut scanner);
    let errors = scanner.into_errors();
    match result {
        Ok(value) if errors.is_empty() => expect.assert_eq(&f(&value)),
        Ok(value) => expect.assert_eq(&format!("{}\n\n{errors:#?}", f(&value))),
        Err(error) if errors.is_empty() => expect.assert_debug_eq(&error),
        Err(error) => expect.assert_eq(&format!("{error:#?}\n\n{errors:#?}")),
    }
}

#[test]
fn test_completion_end_of_keyword() {
    let input = "namespace Foo { open ".to_string();
    let cursor = 20;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    assert_eq!(format!("{:?}", v), "[Keyword(\"internal\"), Keyword(\"open\"), Keyword(\"newtype\"), Keyword(\"function\"), Keyword(\"operation\")]");
}

#[test]
fn test_completion_after_open() {
    let input = "namespace Foo { open ".to_string();
    let cursor = 21;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    // a namespace follows the open keyword
    assert_eq!(format!("{:?}", v), "[Namespace]");
}

#[test]
fn test_completion_begin_ident() {
    let input = "namespace Foo { open X".to_string();
    let cursor = 21;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    // right at the beginning of the namespace name.
    assert_eq!(format!("{:?}", v), "[Namespace]");
}

#[test]
fn test_completion_middle_ident() {
    let input = "namespace Foo { open ABCD".to_string();
    let cursor = 23;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    // middle of the namespace name
    assert_eq!(format!("{:?}", v), "[Namespace]");
}

#[test]
fn test_completion_end_ident() {
    let input = "namespace Foo { open ABCD ".to_string();
    let cursor = 25;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    // end of the namespace name
    assert_eq!(format!("{:?}", v), "[Namespace]");
}

#[test]
fn test_completion_middle() {
    let input = "namespace Foo { open ABCD; open Foo; operation Main() : Unit {} }".to_string();
    let cursor = 23;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    assert_eq!(format!("{:?}", v), "[Namespace]");
}

#[test]
fn test_completion_lotsawhitespace() {
    let input =
        r#"namespace MyQuantumApp { open Microsoft.Quantum.Diagnostics;      }"#.to_string();
    let cursor = 61;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    assert_eq!(format!("{:?}", v), "[Keyword(\"internal\"), Keyword(\"open\"), Keyword(\"newtype\"), Keyword(\"function\"), Keyword(\"operation\")]");
}

#[test]
fn test_completion_after_semicolon() {
    let input =
        r#"namespace MyQuantumApp { open Microsoft.Quantum.Diagnostics;      }"#.to_string();
    let cursor = 60;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    assert_eq!(format!("{:?}", v), "[Keyword(\"internal\"), Keyword(\"open\"), Keyword(\"newtype\"), Keyword(\"function\"), Keyword(\"operation\")]");
}

#[test]
fn test_completion_before_attr() {
    let input =
        r#"namespace Foo { open Microsoft.Quantum.Diagnostics;          @EntryPoint() operation Main() : Unit {} }"#.to_string();
    let cursor = 55;
    let mut scanner = Scanner::predict_mode(&input, cursor as u32);
    let _ = crate::item::parse_namespaces(&mut scanner);
    let v = scanner.last_expected();

    assert_eq!(format!("{:?}", v), "[Keyword(\"internal\"), Keyword(\"open\"), Keyword(\"newtype\"), Keyword(\"function\"), Keyword(\"operation\")]");
}
