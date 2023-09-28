// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

//! The first lexing phase transforms an input string into literals, single-character operators,
//! whitespace, and comments. Keywords are treated as identifiers. The raw token stream is
//! contiguous: there are no gaps between tokens.
//!
//! These are "raw" tokens because single-character operators don't always correspond to Q#
//! operators, and whitespace and comments will later be discarded. Raw tokens are the ingredients
//! that are "cooked" into compound tokens before they can be consumed by the parser.
//!
//! Tokens never contain substrings from the original input, but are simply labels that refer back
//! to offsets in the input. Lexing never fails, but may produce unknown tokens.

#[cfg(test)]
mod tests;

use super::{Delim, InterpolatedEnding, InterpolatedStart, Radix};
use enum_iterator::Sequence;
use owned_chars::OwnedCharIndices;
use std::{
    fmt::{self, Display, Formatter, Write},
    iter::Peekable,
    str::CharIndices,
    sync::Arc,
};

/// A raw token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Token {
    /// The token kind.
    pub(super) kind: TokenKind,
    /// The byte offset of the token starting character.
    pub(super) offset: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub(crate) enum TokenKind {
    Comment(CommentKind),
    Ident,
    Number(Number),
    Single(Single),
    String(StringToken),
    Unknown,
    Whitespace,
}

impl Display for TokenKind {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            TokenKind::Comment(CommentKind::Normal) => f.write_str("comment"),
            TokenKind::Comment(CommentKind::Doc) => f.write_str("doc comment"),
            TokenKind::Ident => f.write_str("identifier"),
            TokenKind::Number(Number::BigInt(_)) => f.write_str("big integer"),
            TokenKind::Number(Number::Float) => f.write_str("float"),
            TokenKind::Number(Number::Int(_)) => f.write_str("integer"),
            TokenKind::Single(single) => write!(f, "`{single}`"),
            TokenKind::String(_) => f.write_str("string"),
            TokenKind::Unknown => f.write_str("unknown"),
            TokenKind::Whitespace => f.write_str("whitespace"),
        }
    }
}

/// A single-character operator token.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub(crate) enum Single {
    /// `&`
    Amp,
    /// `'`
    Apos,
    /// `@`
    At,
    /// `!`
    Bang,
    /// `|`
    Bar,
    /// `^`
    Caret,
    /// A closing delimiter.
    Close(Delim),
    /// `:`
    Colon,
    /// `,`
    Comma,
    /// `.`
    Dot,
    /// `=`
    Eq,
    /// `>`
    Gt,
    /// `<`
    Lt,
    /// `-`
    Minus,
    /// An opening delimiter.
    Open(Delim),
    /// `%`
    Percent,
    /// `+`
    Plus,
    /// `?`
    Question,
    /// `;`
    Semi,
    /// `/`
    Slash,
    /// `*`
    Star,
    /// `~`
    Tilde,
}

impl Display for Single {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        f.write_char(match self {
            Single::Amp => '&',
            Single::Apos => '\'',
            Single::At => '@',
            Single::Bang => '!',
            Single::Bar => '|',
            Single::Caret => '^',
            Single::Close(Delim::Brace) => '}',
            Single::Close(Delim::Bracket) => ']',
            Single::Close(Delim::Paren) => ')',
            Single::Colon => ':',
            Single::Comma => ',',
            Single::Dot => '.',
            Single::Eq => '=',
            Single::Gt => '>',
            Single::Lt => '<',
            Single::Minus => '-',
            Single::Open(Delim::Brace) => '{',
            Single::Open(Delim::Bracket) => '[',
            Single::Open(Delim::Paren) => '(',
            Single::Percent => '%',
            Single::Plus => '+',
            Single::Question => '?',
            Single::Semi => ';',
            Single::Slash => '/',
            Single::Star => '*',
            Single::Tilde => '~',
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub(crate) enum Number {
    BigInt(Radix),
    Float,
    Int(Radix),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub(crate) enum StringToken {
    Normal { terminated: bool },
    Interpolated(InterpolatedStart, Option<InterpolatedEnding>),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StringKind {
    Normal,
    Interpolated,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Sequence)]
pub(crate) enum CommentKind {
    Normal,
    Doc,
}

pub(super) struct Lexer {
    peek: [Option<Token>; 2],
    chars: OwnedCharIndices,
    interpolation: u8,
}

impl Lexer {
    pub(super) fn next_if(
        &mut self,
        func: impl FnOnce(&<Self as Iterator>::Item) -> bool,
    ) -> Option<<Self as Iterator>::Item> {
        let peek = self.peek();
        match peek {
            Some(matched) if func(&matched) => {
                // advance the real iterator since we matched
                self.next()
            }
            _ => None,
        }
    }

    pub(super) fn peek(&mut self) -> Option<<Self as Iterator>::Item> {
        let peek = self.peek.clone();
        match peek {
            [Some(val), _] => Some(val),
            [_, Some(val)] => {
                self.peek = [Some(val.clone()), None];
                Some(val)
            }
            [None, None] => {
                let next = self.next();
                self.peek = [next.clone(), None];
                next
            }
        }
    }

    pub(crate) fn peek_second(&mut self) -> Option<<Self as Iterator>::Item> {
        let peek = self.peek.clone();
        match peek {
            [Some(_la1), Some(la2)] => Some(la2.clone()),
            [None, Some(la1)] => {
                let la2 = self.next();
                self.peek = [Some(la1), la2.clone()];
                la2
            }
            [Some(la1), None] => {
                let la2 = self.next();
                self.peek = [Some(la1), la2.clone()];
                la2
            }
            [None, None] => {
                let la1 = self.next();
                let la2 = self.next();
                self.peek = [la1.clone(), la2.clone()];
                la2
            }
        }
    }
}

pub(super) trait OwnedPeekable: Iterator {
    fn next_if(
        &mut self,
        func: impl FnOnce(&<Self as Iterator>::Item) -> bool,
    ) -> Option<<Self as Iterator>::Item>;
    fn next_if_eq(
        &mut self,
        expected: &<Self as Iterator>::Item,
    ) -> Option<<Self as Iterator>::Item>;
    fn peek(&self) -> Option<<Self as Iterator>::Item>;
}

impl OwnedPeekable for OwnedCharIndices {
    fn next_if(
        &mut self,
        func: impl FnOnce(&<Self as Iterator>::Item) -> bool,
    ) -> Option<<Self as Iterator>::Item> {
        let mut chars = self.as_str().char_indices();
        match chars.next() {
            Some(matched) if func(&matched) => {
                // advance the real iterator since we matched
                self.next()
            }
            _ => None,
        }
    }

    fn next_if_eq(
        &mut self,
        expected: &<Self as Iterator>::Item,
    ) -> Option<<Self as Iterator>::Item> {
        let mut chars = self.as_str().char_indices();
        match chars.next() {
            Some(matched) if &matched == expected => {
                // advance the real iterator since we matched
                self.next()
            }

            _ => None,
        }
    }

    fn peek(&self) -> Option<<Self as Iterator>::Item> {
        let mut chars = self.as_str().char_indices().peekable();
        chars.peek().cloned()
    }
}

impl Lexer {
    pub(super) fn new(input: Arc<str>) -> Self {
        Self {
            peek: [None, None],
            chars: OwnedCharIndices::from_string(input.to_string()),
            interpolation: 0,
        }
    }

    fn next_if_eq(&mut self, c: char) -> bool {
        self.chars.next_if(|i| i.1 == c).is_some()
    }

    fn eat_while(&mut self, mut f: impl FnMut(char) -> bool) {
        while self.chars.next_if(|i| f(i.1)).is_some() {}
    }

    /// Returns the first character ahead of the cursor without consuming it. This operation is fast,
    /// but if you know you want to consume the character if it matches, use [`next_if_eq`] instead.
    fn first(&mut self) -> Option<char> {
        self.chars.peek().map(|i| i.1)
    }

    /// Returns the second character ahead of the cursor without consuming it. This is slower
    /// than [`first`] and should be avoided when possible.
    fn second(&self) -> Option<char> {
        let mut chars = self.as_char_indices();
        chars.next();
        chars.next().map(|i| i.1)
    }

    fn whitespace(&mut self, c: char) -> bool {
        if c.is_whitespace() {
            self.eat_while(char::is_whitespace);
            true
        } else {
            false
        }
    }

    fn comment(&mut self, c: char) -> Option<CommentKind> {
        if c == '/' && self.next_if_eq('/') {
            let kind = if self.first() == Some('/') && self.second() != Some('/') {
                self.chars.next();
                CommentKind::Doc
            } else {
                CommentKind::Normal
            };

            self.eat_while(|c| c != '\n');
            Some(kind)
        } else {
            None
        }
    }

    fn ident(&mut self, c: char) -> bool {
        if c == '_' || c.is_alphabetic() {
            self.eat_while(|c| c == '_' || c.is_alphanumeric());
            true
        } else {
            false
        }
    }

    fn number(&mut self, c: char) -> Option<Number> {
        self.leading_zero(c).or_else(|| self.decimal(c))
    }

    fn leading_zero(&mut self, c: char) -> Option<Number> {
        if c != '0' {
            return None;
        }

        let radix = if self.next_if_eq('b') {
            Radix::Binary
        } else if self.next_if_eq('o') {
            Radix::Octal
        } else if self.next_if_eq('x') {
            Radix::Hexadecimal
        } else {
            Radix::Decimal
        };

        self.eat_while(|c| c == '_' || c.is_digit(radix.into()));
        if self.next_if_eq('L') {
            Some(Number::BigInt(radix))
        } else if radix == Radix::Decimal && self.float() {
            Some(Number::Float)
        } else {
            Some(Number::Int(radix))
        }
    }

    fn decimal(&mut self, c: char) -> Option<Number> {
        if !c.is_ascii_digit() {
            return None;
        }

        self.eat_while(|c| c == '_' || c.is_ascii_digit());

        if self.float() {
            Some(Number::Float)
        } else if self.next_if_eq('L') {
            Some(Number::BigInt(Radix::Decimal))
        } else {
            Some(Number::Int(Radix::Decimal))
        }
    }

    fn float(&mut self) -> bool {
        // Watch out for ranges: `0..` should be an integer followed by two dots.
        if self.first() == Some('.') && self.second() != Some('.') {
            self.chars.next();
            self.eat_while(|c| c == '_' || c.is_ascii_digit());
            self.exp();
            true
        } else {
            self.exp()
        }
    }

    fn exp(&mut self) -> bool {
        if self.next_if_eq('e') {
            self.chars.next_if(|i| i.1 == '+' || i.1 == '-');
            self.eat_while(|c| c.is_ascii_digit());
            true
        } else {
            false
        }
    }

    fn string(&mut self, c: char) -> Option<TokenKind> {
        let kind = self.start_string(c)?;

        while self
            .first()
            .map_or(false, |c| !is_string_terminator(kind, c))
        {
            self.eat_while(|c| c != '\\' && !is_string_terminator(kind, c));
            if self.next_if_eq('\\') {
                self.chars.next();
            }
        }

        Some(TokenKind::String(self.finish_string(c, kind)))
    }

    fn start_string(&mut self, c: char) -> Option<StringKind> {
        if c == '$' {
            if self.next_if_eq('"') {
                Some(StringKind::Interpolated)
            } else {
                None
            }
        } else if c == '"' {
            Some(StringKind::Normal)
        } else if self.interpolation > 0 && c == '}' {
            self.interpolation = self
                .interpolation
                .checked_sub(1)
                .expect("interpolation level should have been incremented at left brace");
            Some(StringKind::Interpolated)
        } else {
            None
        }
    }

    fn finish_string(&mut self, start: char, kind: StringKind) -> StringToken {
        match kind {
            StringKind::Normal => StringToken::Normal {
                terminated: self.next_if_eq('"'),
            },
            StringKind::Interpolated => {
                let start = if start == '$' {
                    InterpolatedStart::DollarQuote
                } else {
                    InterpolatedStart::RBrace
                };

                let end = if self.next_if_eq('{') {
                    self.interpolation = self
                        .interpolation
                        .checked_add(1)
                        .expect("interpolation should not exceed maximum depth");
                    Some(InterpolatedEnding::LBrace)
                } else if self.next_if_eq('"') {
                    Some(InterpolatedEnding::Quote)
                } else {
                    None // Unterminated string.
                };

                StringToken::Interpolated(start, end)
            }
        }
    }

    /// This functions like a clone of the iterator, but not the underlying
    /// data. We clone an iterator that references the original string, and
    /// generate a char indices iter struct from that, but this function does not
    /// clone the original data.
    pub(super) fn as_char_indices<'a>(&'a self) -> CharIndices<'a> {
        self.chars.as_str().char_indices()
    }
}

impl Iterator for Lexer {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        // self.peek.pop();
        let peek = self.peek.clone();
        match peek {
            // if we have either a peek in the lookahead 1 or lookahead 2 position
            // return that peek and empty the queue
            [None, Some(la)] | [Some(la), None] => {
                self.peek = [None, None];
                return Some(la);
            }
            // if we have looked ahead both 1 and 2, then pop
            // lookahead 1 and move lookahead 2 up
            [Some(la_1), Some(la_2)] => {
                self.peek = [Some(la_2.clone()), None];
                return Some(la_1);
            }
            // if we have nothing in the lookahead queue, advance the iterator.
            [None, None] => (),
        }
        let (offset, c) = self.chars.next()?;
        let kind = if let Some(kind) = self.comment(c) {
            TokenKind::Comment(kind)
        } else if self.whitespace(c) {
            TokenKind::Whitespace
        } else if self.ident(c) {
            TokenKind::Ident
        } else {
            self.number(c)
                .map(TokenKind::Number)
                .or_else(|| self.string(c))
                .or_else(|| single(c).map(TokenKind::Single))
                .unwrap_or(TokenKind::Unknown)
        };
        // println!("self.offset: {}\ntoken offset: {}", self.offset, offset);
        Some(Token {
            kind,
            offset: offset.try_into().expect("offset should fit into u32"),
        })
    }
}

fn single(c: char) -> Option<Single> {
    match c {
        '-' => Some(Single::Minus),
        ',' => Some(Single::Comma),
        ';' => Some(Single::Semi),
        ':' => Some(Single::Colon),
        '!' => Some(Single::Bang),
        '?' => Some(Single::Question),
        '.' => Some(Single::Dot),
        '\'' => Some(Single::Apos),
        '(' => Some(Single::Open(Delim::Paren)),
        ')' => Some(Single::Close(Delim::Paren)),
        '[' => Some(Single::Open(Delim::Bracket)),
        ']' => Some(Single::Close(Delim::Bracket)),
        '{' => Some(Single::Open(Delim::Brace)),
        '}' => Some(Single::Close(Delim::Brace)),
        '@' => Some(Single::At),
        '*' => Some(Single::Star),
        '/' => Some(Single::Slash),
        '&' => Some(Single::Amp),
        '%' => Some(Single::Percent),
        '^' => Some(Single::Caret),
        '+' => Some(Single::Plus),
        '<' => Some(Single::Lt),
        '=' => Some(Single::Eq),
        '>' => Some(Single::Gt),
        '|' => Some(Single::Bar),
        '~' => Some(Single::Tilde),
        _ => None,
    }
}

fn is_string_terminator(kind: StringKind, c: char) -> bool {
    c == '"' || kind == StringKind::Interpolated && c == '{'
}
