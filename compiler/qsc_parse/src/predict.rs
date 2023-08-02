// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

pub(super) mod wildcard;

use crate::lex::Token;

use self::wildcard::{LexerW, State};

#[derive(Clone, Debug, PartialEq)]
pub enum Prediction {
    Path,
    Field,
    Attr,
    Namespace,
    Qubit,
    Ty,
    TyParam,
    Keyword(&'static str),

    // Keep the below around just for debugging
    Debug(String), // arbitrary debug string to stick in list
    Other(String), // Some other token kind that we don't care about
}

pub(crate) struct PredictionLexer<'a> {
    tokens: LexerW<'a>,
    predictions: Vec<Prediction>,
    state: State,
}

impl<'a> PredictionLexer<'a> {
    pub fn new(input: &'a str, cursor_offset: u32) -> Self {
        PredictionLexer {
            tokens: LexerW::new(input, cursor_offset),
            predictions: Vec::new(),
            state: State::Normal,
        }
    }

    pub fn push_prediction(&mut self, expectations: Vec<Prediction>) {
        println!("received predictions: {:?}", expectations);
        if self.state == State::Wildcard {
            println!("at wildcard, pushed predictions");
            self.predictions.extend(expectations)
        }
    }

    pub fn end(&mut self) {
        println!("stopped collecting");
        self.state = State::End;
    }

    pub fn into_predictions(self) -> Vec<Prediction> {
        self.predictions
    }
}

impl Iterator for PredictionLexer<'_> {
    type Item = Result<Token, crate::lex::Error>;

    fn next(&mut self) -> Option<Result<Token, crate::lex::Error>> {
        let (result, state) = match self.tokens.next() {
            Some(t) => match t {
                crate::predict::wildcard::TokenW::Token(token) => (Some(Ok(token)), State::Normal),
                crate::predict::wildcard::TokenW::Wildcard => (None, State::Wildcard),
                crate::predict::wildcard::TokenW::Error(err) => (Some(Err(err)), State::Normal),
            },
            None => (None, State::End),
        };
        println!("setting state to {:?}", state);
        self.state = state;
        // No value in collecting errors, but we could do it...
        result
    }
}
