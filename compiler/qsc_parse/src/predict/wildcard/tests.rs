// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::LexerW;
use expect_test::expect;

#[test]
fn wildcard_in_ident() {
    let actual: Vec<_> = LexerW::new("hello", 1).collect();
    expect![[r#"
        [
            Wildcard,
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn wildcard_in_whitespace() {
    let actual: Vec<_> = LexerW::new("hi     there", 5).collect();
    expect![[r#"
        [
            Token(
                Token {
                    kind: Ident,
                    span: Span {
                        lo: 0,
                        hi: 2,
                    },
                },
            ),
            Wildcard,
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn wildcard_between_ops() {
    let actual: Vec<_> = LexerW::new("foo()", 4).collect();
    expect![[r#"
        [
            Token(
                Token {
                    kind: Ident,
                    span: Span {
                        lo: 0,
                        hi: 3,
                    },
                },
            ),
            Token(
                Token {
                    kind: Open(
                        Paren,
                    ),
                    span: Span {
                        lo: 3,
                        hi: 4,
                    },
                },
            ),
            Wildcard,
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn wildcard_at_eof() {
    let actual: Vec<_> = LexerW::new("(", 1).collect();
    expect![[r#"
        [
            Token(
                Token {
                    kind: Open(
                        Paren,
                    ),
                    span: Span {
                        lo: 0,
                        hi: 1,
                    },
                },
            ),
            Wildcard,
        ]
    "#]]
    .assert_debug_eq(&actual);
}

#[test]
fn wildcard_in_ident_eof() {
    let actual: Vec<_> = LexerW::new("hello", 5).collect();
    expect![[r#"
        [
            Wildcard,
        ]
    "#]]
    .assert_debug_eq(&actual);
}
