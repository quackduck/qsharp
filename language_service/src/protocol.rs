// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc::utf16::{Position as utf16_Position, PositionEncoding as utf16_PositionEncoding};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Position {
    /// Line and column are both 0-indexed
    /// Column offset in terms of utf-16 code units (i.e. 2-byte units)
    Utf16LineColumn(utf16_Position),
    /// Offset in terms of utf-8 code units (bytes)
    Utf8Offset(u32),
}

impl Position {
    #[must_use]
    pub fn utf16_line_column(line: u32, column: u32) -> Self {
        Position::Utf16LineColumn(utf16_Position {
            encoding: utf16_PositionEncoding::Utf16,
            line,
            column,
        })
    }
}

/// Represents a span of text used by the Language Server API
#[derive(Debug, PartialEq)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

#[derive(Copy, Clone, Debug, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub enum CompletionItemKind {
    // It would have been nice to match these enum values to the ones used by
    // VS Code and Monaco, but unfortunately those two disagree on the values.
    // So we define our own unique enum here to reduce confusion.
    Function,
    Interface,
    Keyword,
    Module,
}

#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct CompletionList {
    pub items: Vec<CompletionItem>,
}

#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
    pub sort_text: Option<String>,
    pub detail: Option<String>,
    pub additional_text_edits: Option<Vec<(Span, String)>>,
}

impl CompletionItem {
    #[must_use]
    pub fn new(label: String, kind: CompletionItemKind) -> Self {
        CompletionItem {
            label,
            kind,
            sort_text: None,
            detail: None,
            additional_text_edits: None,
        }
    }
}

#[derive(Debug, PartialEq)]
pub struct Definition {
    pub source: String,
    pub position: Position,
}

#[derive(Debug, PartialEq)]
pub struct Hover {
    pub contents: String,
    pub span: Span,
}
