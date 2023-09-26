// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use super::Error;
use crate::{
    lex::{Lexer, Token, TokenKind},
    ErrorKind,
};
use qsc_data_structures::span::Span;

#[derive(Debug)]
pub(super) struct NoBarrierError;

/// Scans over the token stream. Notably enforces LL(1) parser behavior via
/// its lack of a [Clone] implementation and limited peek functionality.
/// This struct should never be clonable, and it should never be able to
/// peek more than one token ahead, to maintain LL(1) enforcement.
pub(super) struct Scanner<'a> {
    input: Vec<&'a str>,
    // the index of the input field on this struct
    // for the module we are currently parsing
    current_module_index: usize,
    tokens: Lexer<'a>,
    barriers: Vec<&'a [TokenKind]>,
    errors: Vec<Error>,
    recovered_eof: bool,
    peek: Token,
    offset: u32,
}

impl<'a> Scanner<'a> {
    pub(super) fn new(input: &'a str) -> Self {
        let mut tokens = Lexer::new(input);
        let (peek, errors) = next_ok(&mut tokens);
        Self {
            input: vec![input],
            current_module_index: 0,
            tokens,
            barriers: Vec::new(),
            errors: errors
                .into_iter()
                .map(|e| Error(ErrorKind::Lex(e)))
                .collect(),
            recovered_eof: false,
            peek: peek.unwrap_or_else(|| eof(input.len())),
            offset: 0,
        }
    }

    pub(super) fn peek(&self) -> Token {
        self.peek
    }

    pub(super) fn read(&self) -> &str {
        &(*self.input[self.current_module_index])[self.peek.span]
    }

    pub(super) fn span(&self, from: u32) -> Span {
        Span {
            lo: from,
            hi: self.offset,
        }
    }

    fn input(&self) -> &'a str {
        self.input[self.current_module_index]
    }

    pub(super) fn advance(&mut self) {
        if self.peek.kind != TokenKind::Eof {
            self.offset = self.peek.span.hi;
            let (peek, errors) = next_ok(&mut self.tokens);
            self.errors
                .extend(errors.into_iter().map(|e| Error(ErrorKind::Lex(e))));
            // progress to the next module in the queue if we are done parsing this one
            self.peek = match peek {
                Some(tok) => tok,
                None => {
                    // are we out of modules?
                    if self.current_module_index >= (self.input.len() - 1) {
                        eof(self.input().len())
                    } else {
                        // if we still have another module to parse, reset the
                        // Scanner on the next module.
                        self.current_module_index += 1;
                        let mut next_module_tokens = Lexer::new(&*(self.input()));
                        let (peek, errors) = next_ok(&mut next_module_tokens);
                        self.barriers = Vec::new();
                        self.offset = 0;
                        self.tokens = next_module_tokens;
                        self.errors.append(
                            &mut errors
                                .into_iter()
                                .map(|e| Error(ErrorKind::Lex(e)))
                                .collect::<Vec<_>>(),
                        );
                        peek.unwrap_or_else(|| eof(self.input().len()))
                    }
                }
            }
        }
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
        let is_eof_err = matches!(
            error.0,
            ErrorKind::Token(_, TokenKind::Eof, _) | ErrorKind::Rule(_, TokenKind::Eof, _)
        );
        if !is_eof_err || !self.recovered_eof {
            self.errors.push(error);
            self.recovered_eof = self.recovered_eof || is_eof_err;
        }
    }

    pub(super) fn into_errors(self) -> Vec<Error> {
        self.errors
    }

    pub(super) fn push_module(&mut self, module_source: &'a str) {
        self.input.push(module_source)
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
