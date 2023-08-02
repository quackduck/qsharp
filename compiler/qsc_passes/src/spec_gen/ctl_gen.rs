// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use miette::Diagnostic;
use qsc_data_structures::span::Span;
use qsc_hir::{
    hir::{CallableKind, Expr, ExprKind, Functor, NodeId, Res, UnOp},
    mut_visit::{walk_expr, MutVisitor},
    ty::{Arrow, Prim, Ty},
};
use thiserror::Error;

#[derive(Clone, Debug, Diagnostic, Error)]
pub enum Error {
    #[error("operation does not support the controlled functor")]
    #[diagnostic(help("each operation called inside an operation with compiler-generated controlled specializations must support the controlled functor"))]
    #[diagnostic(code("Qsc.CtlGen.MissingCtlFunctor"))]
    MissingCtlFunctor(#[label] Span),
}

pub(super) struct CtlDistrib {
    pub(super) ctls: Res,
    pub(super) errors: Vec<Error>,
}

impl MutVisitor for CtlDistrib {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Call(op, args) => {
                match &op.ty {
                    Ty::Arrow(arrow) if arrow.kind == CallableKind::Operation => {
                        let functors = arrow
                            .functors
                            .expect_value("arrow type should have concrete functors");

                        if functors.contains(&Functor::Ctl) {
                            op.kind = ExprKind::UnOp(UnOp::Functor(Functor::Ctl), op.clone());
                            op.id = NodeId::default();
                            op.ty = Ty::Arrow(Box::new(Arrow {
                                kind: CallableKind::Operation,
                                input: Box::new(Ty::Tuple(vec![
                                    Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                    Ty::clone(&arrow.input),
                                ])),
                                output: arrow.output.clone(),
                                functors: arrow.functors,
                            }));

                            args.kind = ExprKind::Tuple(vec![
                                Expr {
                                    id: NodeId::default(),
                                    span: args.span,
                                    ty: Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                    kind: ExprKind::Var(self.ctls, Vec::new()),
                                },
                                Expr::clone(args),
                            ]);
                            args.ty = Ty::Tuple(vec![
                                Ty::Array(Box::new(Ty::Prim(Prim::Qubit))),
                                Ty::clone(&args.ty),
                            ]);
                            args.id = NodeId::default();
                        } else {
                            self.errors.push(Error::MissingCtlFunctor(op.span));
                        }
                    }
                    _ => {}
                }

                walk_expr(self, expr);
            }
            ExprKind::Conjugate(_, apply) => {
                // Only transform the apply block, the within block can remain as-is.
                self.visit_block(apply);
            }
            _ => walk_expr(self, expr),
        }
    }
}
