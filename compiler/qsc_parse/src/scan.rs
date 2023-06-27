// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Error;
use crate::{
    lex::{Lexer, Token, TokenKind},
    CompletionConstraint, ErrorKind,
};
use qsc_data_structures::span::Span;

#[derive(Debug)]
pub(super) struct NoBarrierError;

struct CompletionFilter {
    cursor_offset: u32,
    at_cursor: bool,
    last_expected: Option<(u32, Vec<CompletionConstraint>)>,
}

pub(super) struct Scanner<'a> {
    input: &'a str,
    tokens: Lexer<'a>,
    barriers: Vec<&'a [TokenKind]>,
    errors: Vec<Error>,
    peek: Token,
    offset: u32,
    completion_mode: Option<CompletionFilter>,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        let mut tokens = Lexer::new(input);
        let (peek, errors) = next_ok(&mut tokens);
        Self {
            input,
            tokens,
            barriers: Vec::new(),
            errors: errors
                .into_iter()
                .map(|e| Error(ErrorKind::Lex(e)))
                .collect(),
            peek: peek.unwrap_or_else(|| eof(input.len())),
            offset: 0,
            completion_mode: None,
        }
    }

    pub(super) fn completion_mode(input: &'a str, cursor_offset: u32) -> Self {
        let mut tokens = Lexer::new(input);
        let (peek, errors) = next_ok(&mut tokens);
        Self {
            input,
            tokens,
            barriers: Vec::new(),
            errors: errors
                .into_iter()
                .map(|e| Error(ErrorKind::Lex(e)))
                .collect(),
            peek: peek.unwrap_or_else(|| eof(input.len())),
            offset: 0,
            completion_mode: Some(CompletionFilter {
                at_cursor: cursor_offset == 0,
                cursor_offset,
                last_expected: None,
            }),
        }
    }

    pub(super) fn push_expectation(&mut self, expectations: Vec<CompletionConstraint>) {
        if let Some(filter) = &mut self.completion_mode {
            println!(
                "expecting {:?} at ({},{}] ",
                expectations, self.offset, self.peek.span.hi
            );

            if filter.at_cursor {
                println!("  recorded");
                filter
                    .last_expected
                    // kill this value it's misleading
                    .get_or_insert_with(|| (9999, Vec::new()))
                    .1
                    .extend(expectations)
            }
        }
    }

    pub(super) fn last_expected(&self) -> Option<(u32, Vec<CompletionConstraint>)> {
        // avoid copy at some point... or don't, whatever, this gets called once
        self.completion_mode
            .as_ref()
            .expect("don't call into_expected if you're not in completion mode")
            .last_expected
            .clone()
    }

    pub(super) fn peek(&self) -> Token {
        self.peek
    }

    pub(super) fn read(&self) -> &'a str {
        &self.input[self.peek.span]
    }

    pub(super) fn span(&self, from: u32) -> Span {
        Span {
            lo: from,
            hi: self.offset,
        }
    }

    pub(super) fn advance(&mut self) {
        print!("advancing {} -> ", self.offset);
        if self.peek.kind != TokenKind::Eof {
            self.offset = self.peek.span.hi;
            let (peek, errors) = next_ok(&mut self.tokens);
            self.errors
                .extend(errors.into_iter().map(|e| Error(ErrorKind::Lex(e))));
            self.peek = peek.unwrap_or_else(|| eof(self.input.len()));

            if let Some(c) = &mut self.completion_mode {
                if self.offset < c.cursor_offset && self.peek.span.hi >= c.cursor_offset {
                    // did we hit the completion cursor? start collecting expectations.
                    if self.peek.span.hi == c.cursor_offset && !is_word_or_eof(self.peek.kind) {
                        // if cursor is touching the end of a token, only count if it's eof (because it's 0-length)
                        // or if it's a word. If it's not a word then let's move the cursor over one so that it will
                        // be counted as part of the next token.
                        c.cursor_offset += 1;
                    } else {
                        c.at_cursor = true;
                        // pretend we're at eof because we don't
                        // want the parser to find the next (valid) token
                        // and stop trying possibilities.
                        self.peek = eof(c.cursor_offset as usize);
                    }
                }
            }
        }
        // offset is the end of the last token, peek.span.lo is the beginning of the current token
        println!(
            "{} ({}) {}",
            self.offset,
            self.peek.span.lo,
            self.completion_mode
                .as_ref()
                .map_or("", |c| if c.at_cursor { "at cursor!" } else { "" })
        );
    }

    /// Pushes a recovery barrier. While the barrier is active, recovery will never advance past any
    /// of the barrier tokens, unless it is explicitly listed as a recovery token.
    pub(super) fn push_barrier(&mut self, tokens: &'a [TokenKind]) {
        self.barriers.push(tokens);
    }

    /// Pops the most recently pushed active barrier.
    pub(super) fn pop_barrier(&mut self) -> Result<(), NoBarrierError> {
        match self.barriers.pop() {
            Some(_) => Ok(()),
            None => Err(NoBarrierError),
        }
    }

    /// Tries to recover from a parse error by advancing tokens until any of the given recovery
    /// tokens, or a barrier token, is found. If a recovery token is found, it is consumed. If a
    /// barrier token is found first, it is not consumed.
    pub(super) fn recover(&mut self, tokens: &[TokenKind]) {
        println!("recovering at {} ", self.peek.span.lo);
        if let Some(c) = &mut self.completion_mode {
            if c.at_cursor {
                println!("stopped collecting");
                // Once we've found the cursor, we don't want to keep collecting expectations
                // after error recovery. If we've just done recovery
                // at the cursor then the expected tokens aren't gonna be that helpful.
                c.at_cursor = false;
            }
        }
        loop {
            let peek = self.peek.kind;
            if contains(peek, tokens) {
                self.advance();
                break;
            } else if peek == TokenKind::Eof || self.barriers.iter().any(|&b| contains(peek, b)) {
                break;
            } else {
                self.advance();
            }
        }
    }

    pub(super) fn push_error(&mut self, error: Error) {
        self.errors.push(error);
    }

    pub(super) fn into_errors(self) -> Vec<Error> {
        self.errors
    }
}

fn eof(offset: usize) -> Token {
    let offset = offset.try_into().expect("eof offset should fit into u32");
    Token {
        kind: TokenKind::Eof,
        span: Span {
            lo: offset,
            hi: offset,
        },
    }
}

/// Advances the iterator by skipping [`Err`] values until the first [`Ok`] value is found. Returns
/// the found value or [`None`] if the iterator is exhausted. All skipped errors are also
/// accumulated into a vector and returned.
fn next_ok<T, E>(iter: impl Iterator<Item = Result<T, E>>) -> (Option<T>, Vec<E>) {
    let mut errors = Vec::new();
    for result in iter {
        match result {
            Ok(v) => return (Some(v), errors),
            Err(e) => errors.push(e),
        }
    }

    (None, errors)
}

fn contains<'a>(token: TokenKind, tokens: impl IntoIterator<Item = &'a TokenKind>) -> bool {
    tokens.into_iter().any(|&t| t == token)
}

fn is_word_or_eof(t: TokenKind) -> bool {
    if let TokenKind::Keyword(_) = t {
        return true;
    }
    t == TokenKind::Ident || t == TokenKind::Eof
}
