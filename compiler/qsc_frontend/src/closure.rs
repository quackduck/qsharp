// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_hir::{
    assigner::Assigner,
    hir::{
        Block, CallableDecl, CallableKind, Expr, ExprKind, Ident, Mutability, NodeId, Pat, PatKind,
        Res, SpecBody, SpecDecl, Stmt, StmtKind,
    },
    mut_visit::{self, MutVisitor},
    ty::{Arrow, FunctorSetValue, Ty},
    visit::{self, Visitor},
};
use std::{
    collections::{HashMap, HashSet},
    iter,
};

pub(super) struct Lambda {
    pub(super) kind: CallableKind,
    pub(super) functors: FunctorSetValue,
    pub(super) input: Pat,
    pub(super) body: Expr,
}

pub(super) struct PartialApp {
    pub(super) bindings: Vec<Stmt>,
    pub(super) input: Pat,
}

struct VarFinder {
    bindings: HashSet<NodeId>,
    uses: HashSet<NodeId>,
}

impl VarFinder {
    fn free_vars(&self) -> Vec<NodeId> {
        let mut vars: Vec<_> = self.uses.difference(&self.bindings).copied().collect();
        vars.sort_unstable();
        vars
    }
}

impl Visitor<'_> for VarFinder {
    fn visit_expr(&mut self, expr: &Expr) {
        match &expr.kind {
            ExprKind::Closure(args, _) => self.uses.extend(args.iter().copied()),
            &ExprKind::Var(Res::Local(id), _) => {
                self.uses.insert(id);
            }
            _ => visit::walk_expr(self, expr),
        }
    }

    fn visit_pat(&mut self, pat: &Pat) {
        if let PatKind::Bind(name) = &pat.kind {
            self.bindings.insert(name.id);
        } else {
            visit::walk_pat(self, pat);
        }
    }
}

struct VarReplacer<'a> {
    substitutions: &'a HashMap<NodeId, NodeId>,
}

impl VarReplacer<'_> {
    fn replace(&self, id: &mut NodeId) {
        if let Some(&new_id) = self.substitutions.get(id) {
            *id = new_id;
        }
    }
}

impl MutVisitor for VarReplacer<'_> {
    fn visit_expr(&mut self, expr: &mut Expr) {
        match &mut expr.kind {
            ExprKind::Closure(args, _) => args.iter_mut().for_each(|arg| self.replace(arg)),
            ExprKind::Var(Res::Local(id), _) => self.replace(id),
            _ => mut_visit::walk_expr(self, expr),
        }
    }
}

pub(super) fn lift(
    assigner: &mut Assigner,
    locals: &IndexMap<NodeId, (Ident, Ty)>,
    mut lambda: Lambda,
    span: Span,
) -> (Vec<NodeId>, CallableDecl) {
    let mut finder = VarFinder {
        bindings: HashSet::new(),
        uses: HashSet::new(),
    };
    finder.visit_pat(&lambda.input);
    finder.visit_expr(&lambda.body);

    let free_vars = finder.free_vars();
    let substitutions: HashMap<_, _> = free_vars
        .iter()
        .map(|&id| (id, assigner.next_node()))
        .collect();

    VarReplacer {
        substitutions: &substitutions,
    }
    .visit_expr(&mut lambda.body);

    let substituted_vars = free_vars.iter().map(|&id| {
        let &new_id = substitutions
            .get(&id)
            .expect("free variable should have substitution");
        let original_ident = locals
            .get(id)
            .expect("free variable should be a local")
            .clone();
        (new_id, original_ident)
    });

    let mut input = closure_input(substituted_vars, lambda.input, span);
    assigner.visit_pat(&mut input);

    let callable = CallableDecl {
        id: assigner.next_node(),
        span,
        kind: lambda.kind,
        name: Ident {
            id: assigner.next_node(),
            span: Span::default(),
            name: "lambda".into(),
        },
        generics: Vec::new(),
        input,
        output: lambda.body.ty.clone(),
        functors: lambda.functors,
        body: SpecDecl {
            id: assigner.next_node(),
            span: lambda.body.span,
            body: SpecBody::Impl(
                None,
                Block {
                    id: assigner.next_node(),
                    span: lambda.body.span,
                    ty: lambda.body.ty.clone(),
                    stmts: vec![Stmt {
                        id: assigner.next_node(),
                        span: lambda.body.span,
                        kind: StmtKind::Expr(lambda.body),
                    }],
                },
            ),
        },
        adj: None,
        ctl: None,
        ctl_adj: None,
    };

    (free_vars, callable)
}

pub(super) fn partial_app_block(
    close: impl FnOnce(Lambda) -> ExprKind,
    callee: Expr,
    arg: Expr,
    app: PartialApp,
    arrow: Arrow,
    span: Span,
) -> Block {
    let call = Expr {
        id: NodeId::default(),
        span,
        ty: (*arrow.output).clone(),
        kind: ExprKind::Call(Box::new(callee), Box::new(arg)),
    };
    let lambda = Lambda {
        kind: arrow.kind,
        functors: arrow
            .functors
            .expect_value("lambda type should have concrete functors"),
        input: app.input,
        body: call,
    };
    let closure = Expr {
        id: NodeId::default(),
        span,
        ty: Ty::Arrow(Box::new(arrow.clone())),
        kind: close(lambda),
    };

    let mut stmts = app.bindings;
    stmts.push(Stmt {
        id: NodeId::default(),
        span,
        kind: StmtKind::Expr(closure),
    });
    Block {
        id: NodeId::default(),
        span,
        ty: Ty::Arrow(Box::new(arrow)),
        stmts,
    }
}

pub(super) fn partial_app_hole(
    assigner: &mut Assigner,
    locals: &mut IndexMap<NodeId, (Ident, Ty)>,
    ty: Ty,
    span: Span,
) -> (Expr, PartialApp) {
    let local_id = assigner.next_node();
    let ident = Ident {
        id: local_id,
        span,
        name: "hole".into(),
    };

    locals.insert(local_id, (ident.clone(), ty.clone()));

    let app = PartialApp {
        bindings: Vec::new(),
        input: Pat {
            id: assigner.next_node(),
            span,
            ty: ty.clone(),
            kind: PatKind::Bind(ident),
        },
    };

    let var = Expr {
        id: assigner.next_node(),
        span,
        ty,
        kind: ExprKind::Var(Res::Local(local_id), Vec::new()),
    };

    (var, app)
}

pub(super) fn partial_app_given(
    assigner: &mut Assigner,
    locals: &mut IndexMap<NodeId, (Ident, Ty)>,
    arg: Expr,
) -> (Expr, PartialApp) {
    let local_id = assigner.next_node();
    let span = arg.span;
    let ident = Ident {
        id: local_id,
        span,
        name: "arg".into(),
    };

    locals.insert(local_id, (ident.clone(), arg.ty.clone()));

    let var = Expr {
        id: assigner.next_node(),
        span,
        ty: arg.ty.clone(),
        kind: ExprKind::Var(Res::Local(local_id), Vec::new()),
    };

    let binding_pat = Pat {
        id: assigner.next_node(),
        span,
        ty: arg.ty.clone(),
        kind: PatKind::Bind(ident),
    };
    let binding_stmt = Stmt {
        id: assigner.next_node(),
        span,
        kind: StmtKind::Local(Mutability::Immutable, binding_pat, arg),
    };
    let app = PartialApp {
        bindings: vec![binding_stmt],
        input: Pat {
            id: assigner.next_node(),
            span,
            ty: Ty::UNIT,
            kind: PatKind::Tuple(Vec::new()),
        },
    };

    (var, app)
}

pub(super) fn partial_app_tuple(
    args: impl Iterator<Item = (Expr, PartialApp)>,
    span: Span,
) -> (Expr, PartialApp) {
    let mut items = Vec::new();
    let mut bindings = Vec::new();
    let mut holes = Vec::new();
    for (arg, mut app) in args {
        items.push(arg);
        bindings.append(&mut app.bindings);
        if !matches!(&app.input.kind, PatKind::Tuple(items) if items.is_empty()) {
            holes.push(app.input);
        }
    }

    let input = if holes.len() == 1 {
        holes.pop().expect("holes should have one element")
    } else {
        Pat {
            id: NodeId::default(),
            span,
            ty: Ty::Tuple(holes.iter().map(|h| h.ty.clone()).collect()),
            kind: PatKind::Tuple(holes),
        }
    };

    let expr = Expr {
        id: NodeId::default(),
        span,
        ty: Ty::Tuple(items.iter().map(|i| i.ty.clone()).collect()),
        kind: ExprKind::Tuple(items),
    };

    (expr, PartialApp { bindings, input })
}

fn closure_input(
    vars: impl IntoIterator<Item = (NodeId, (Ident, Ty))>,
    input: Pat,
    span: Span,
) -> Pat {
    let bindings: Vec<_> = vars
        .into_iter()
        .map(|(id, (ident, ty))| Pat {
            id: NodeId::default(),
            span: ident.span,
            ty,
            kind: PatKind::Bind(Ident { id, ..ident }),
        })
        .collect();

    let tys = bindings
        .iter()
        .map(|p| p.ty.clone())
        .chain(iter::once(input.ty.clone()))
        .collect();

    Pat {
        id: NodeId::default(),
        span,
        ty: Ty::Tuple(tys),
        kind: PatKind::Tuple(bindings.into_iter().chain(iter::once(input)).collect()),
    }
}
