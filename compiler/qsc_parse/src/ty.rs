// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use super::{
    keyword::Keyword,
    prim::{ident, opt, path, seq, token},
    scan::Scanner,
    Error, Parser, Result,
};
use crate::{
    lex::{ClosedBinOp, Delim, TokenKind},
    prim::keyword,
    ErrorKind, Prediction,
};
use qsc_ast::ast::{
    CallableKind, Functor, FunctorExpr, FunctorExprKind, Ident, NodeId, SetOp, Ty, TyKind,
};

pub(super) fn ty(s: &mut Scanner) -> Result<Ty> {
    let lo = s.peek().span.lo;
    let mut lhs = base(s)?;
    loop {
        if let Some(()) = opt(s, array)? {
            lhs = Ty {
                id: NodeId::default(),
                span: s.span(lo),
                kind: Box::new(TyKind::Array(Box::new(lhs))),
            }
        } else if let Some(kind) = opt(s, arrow)? {
            let output = ty(s)?;
            let functors = if keyword(s, Keyword::Is).is_ok() {
                Some(Box::new(functor_expr(s)?))
            } else {
                None
            };

            lhs = Ty {
                id: NodeId::default(),
                span: s.span(lo),
                kind: Box::new(TyKind::Arrow(
                    kind,
                    Box::new(lhs),
                    Box::new(output),
                    functors,
                )),
            }
        } else {
            break Ok(lhs);
        }
    }
}

pub(super) fn param(s: &mut Scanner) -> Result<Box<Ident>> {
    token(s, TokenKind::Apos)?;
    ident(s)
}

fn array(s: &mut Scanner) -> Result<()> {
    token(s, TokenKind::Open(Delim::Bracket))?;
    token(s, TokenKind::Close(Delim::Bracket))?;
    Ok(())
}

fn arrow(s: &mut Scanner) -> Result<CallableKind> {
    if token(s, TokenKind::RArrow).is_ok() {
        Ok(CallableKind::Function)
    } else if token(s, TokenKind::FatArrow).is_ok() {
        Ok(CallableKind::Operation)
    } else {
        Err(Error(ErrorKind::Rule(
            "arrow type",
            s.peek().kind,
            s.peek().span,
        )))
    }
}

fn base(s: &mut Scanner) -> Result<Ty> {
    let lo = s.peek().span.lo;
    let kind = if keyword(s, Keyword::Underscore).is_ok() {
        Ok(TyKind::Hole)
    } else if let Some(name) = {
        s.push_prediction(vec![Prediction::TyParam]);
        opt(s, param)?
    } {
        Ok(TyKind::Param(name))
    } else if let Some(path) = {
        s.push_prediction(vec![Prediction::Ty]);
        opt(s, path)?
    } {
        Ok(TyKind::Path(path))
    } else if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let (tys, final_sep) = seq(s, ty)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(final_sep.reify(tys, |t| TyKind::Paren(Box::new(t)), TyKind::Tuple))
    } else {
        Err(Error(ErrorKind::Rule("type", s.peek().kind, s.peek().span)))
    }?;

    Ok(Ty {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    })
}

pub(super) fn functor_expr(s: &mut Scanner) -> Result<FunctorExpr> {
    // Intersection binds tighter than union.
    functor_op(s, ClosedBinOp::Plus, SetOp::Union, |s| {
        functor_op(s, ClosedBinOp::Star, SetOp::Intersect, functor_base)
    })
}

fn functor_base(s: &mut Scanner) -> Result<FunctorExpr> {
    let lo = s.peek().span.lo;
    let kind = if token(s, TokenKind::Open(Delim::Paren)).is_ok() {
        let e = functor_expr(s)?;
        token(s, TokenKind::Close(Delim::Paren))?;
        Ok(FunctorExprKind::Paren(Box::new(e)))
    } else if keyword(s, Keyword::Adj).is_ok() {
        Ok(FunctorExprKind::Lit(Functor::Adj))
    } else if keyword(s, Keyword::Ctl).is_ok() {
        Ok(FunctorExprKind::Lit(Functor::Ctl))
    } else {
        Err(Error(ErrorKind::Rule(
            "functor literal",
            s.peek().kind,
            s.peek().span,
        )))
    }?;

    Ok(FunctorExpr {
        id: NodeId::default(),
        span: s.span(lo),
        kind: Box::new(kind),
    })
}

fn functor_op(
    s: &mut Scanner,
    bin_op: ClosedBinOp,
    set_op: SetOp,
    mut p: impl Parser<FunctorExpr>,
) -> Result<FunctorExpr> {
    let lo = s.peek().span.lo;
    let mut lhs = p(s)?;

    while token(s, TokenKind::ClosedBinOp(bin_op)).is_ok() {
        let rhs = p(s)?;
        lhs = FunctorExpr {
            id: NodeId::default(),
            span: s.span(lo),
            kind: Box::new(FunctorExprKind::BinOp(set_op, Box::new(lhs), Box::new(rhs))),
        };
    }

    Ok(lhs)
}
