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

pub(super) struct Scanner<'a> {
    input: &'a str,
    tokens: Lexer<'a>,
    barriers: Vec<&'a [TokenKind]>,
    errors: Vec<Error>,
    peek: Token,
    offset: u32,
    predictions: Option<Vec<CompletionConstraint>>,
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
            predictions: None,
        }
    }

    pub(super) fn predict_mode(input: &'a str, cursor_offset: u32) -> Self {
        let mut tokens = Lexer::with_wildcard(input, cursor_offset);
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
            predictions: Some(Vec::new()),
        }
    }

    pub(super) fn push_expectation(&mut self, expectations: Vec<CompletionConstraint>) {
        if let Some(last_expected) = &mut self.predictions {
            println!(
                "expecting {:?} at ({},{}] ",
                expectations, self.offset, self.peek.span.hi
            );

            if self.peek.kind == TokenKind::Wildcard {
                println!("  recorded");
                last_expected.extend(expectations)
            }
        }
    }

    pub(super) fn last_expected(&self) -> Vec<CompletionConstraint> {
        // avoid copy at some point... or don't, whatever, this gets called once
        return self
            .predictions
            .as_ref()
            .expect("don't call last_expected if you're not in completion mode")
            .clone();
    }

    pub(super) fn peek(&self) -> Token {
        if self.predictions.is_some() && self.peek.kind == TokenKind::Wildcard {
            println!("returning fake eof");
            // pretend we're at eof because we don't
            // want the parser to find the next (valid) token
            // and stop trying possibilities.
            return eof(self.peek.span.lo as usize);
        }
        println!("peeked at {}", self.peek.kind);
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
        }
        // offset is the end of the last token, peek.span.lo is the beginning of the current token
        println!(
            "{} ({}) {}",
            self.offset,
            self.peek.span.lo,
            if self.peek.kind == TokenKind::Wildcard {
                "wildcard!"
            } else {
                ""
            }
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
