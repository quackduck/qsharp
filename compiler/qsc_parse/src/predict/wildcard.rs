// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::lex::{ClosedBinOp, Error, Lexer, Token, TokenKind};

#[cfg(test)]
mod tests;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TokenW {
    Token(Token),
    Wildcard,
    Error(Error),
}

#[derive(PartialEq, Debug)]
pub(crate) enum State {
    Normal,
    Wildcard,
    End,
}

pub(crate) struct LexerW<'a> {
    tokens: Lexer<'a>,
    cursor_offset: u32,
    state: State,
    len: usize,
}

impl<'a> LexerW<'a> {
    pub(crate) fn new(input: &'a str, cursor_offset: u32) -> Self {
        println!("input:\n{input}");
        Self {
            tokens: Lexer::new(input),
            cursor_offset,
            state: if cursor_offset == 0 {
                State::Wildcard
            } else {
                State::Normal
            },
            len: input.len(),
        }
    }
}

impl Iterator for LexerW<'_> {
    type Item = TokenW;

    fn next(&mut self) -> Option<Self::Item> {
        match self.state {
            State::Normal => {
                match self.tokens.next() {
                    Some(next_token) => {
                        match next_token {
                            Ok(token) => {
                                println!(
                                    "token: {:?}-{:?} cursor: {:?}",
                                    token.span.lo, token.span.hi, self.cursor_offset
                                );
                                if token.span.lo >= self.cursor_offset {
                                    // We moved past the cursor already, so cursor was in whitespace, comment, or error token
                                    // The distinction is important, but we'll take care of that later.
                                    // For now assume it was whitespace.
                                    // Insert wildcard, then end
                                    println!("wildcard was in whitespace");
                                    self.state = State::End;
                                    Some(TokenW::Wildcard)
                                } else if token.span.lo < self.cursor_offset
                                    && token.span.hi >= self.cursor_offset
                                {
                                    // Cursor is in the middle or end of the next token.
                                    // word token (ident / keyword / "and" / "or") - drop token, wildcard, then end
                                    // end of non-word token - insert wildcard *after* token, then end
                                    // middle of non-word token (e.g. ==) - no wildcard, end
                                    match token.kind {
                                        TokenKind::Ident
                                        | TokenKind::Keyword(_)
                                        | TokenKind::ClosedBinOp(ClosedBinOp::And)
                                        | TokenKind::ClosedBinOp(ClosedBinOp::Or) => {
                                            println!("wildcard was at end of word");
                                            self.state = State::End;
                                            Some(TokenW::Wildcard)
                                        }
                                        _ => {
                                            if token.span.hi == self.cursor_offset {
                                                println!("wildcard was at end of nonword");
                                                self.state = State::Wildcard;
                                                Some(TokenW::Token(token))
                                            } else {
                                                println!("wildcard was at middle of nonword");
                                                self.state = State::End;
                                                None
                                            }
                                        }
                                    }
                                } else {
                                    // State remains State::Normal
                                    Some(TokenW::Token(token))
                                }
                            }
                            Err(e) => Some(TokenW::Error(e)), // State remains State::Normal (cursor could be in this range, need to handle)
                        }
                    }
                    None => {
                        // We got to the end so presumably the cursor was somewhere after the very last token
                        println!("wildcard at end ({})", self.len);
                        self.state = State::End;
                        Some(TokenW::Wildcard)
                    }
                }
            }
            State::Wildcard => {
                println!("advanced past wildcard");
                self.state = State::End;
                Some(TokenW::Wildcard)
            }
            State::End => {
                println!("at end");
                None
            }
        }
    }
}
