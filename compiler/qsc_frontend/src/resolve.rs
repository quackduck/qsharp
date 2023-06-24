// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use miette::Diagnostic;
use qsc_ast::{
    ast::{self, CallableDecl, Ident, NodeId},
    visit::{self as ast_visit, Visitor as AstVisitor},
};
use qsc_data_structures::{index_map::IndexMap, span::Span};
use qsc_hir::{
    assigner::Assigner,
    global,
    hir::{self, ItemId, LocalItemId, PackageId},
    ty::{ParamId, Prim},
};
use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    rc::Rc,
    vec,
};
use thiserror::Error;

const PRELUDE: &[&str] = &[
    "Microsoft.Quantum.Canon",
    "Microsoft.Quantum.Core",
    "Microsoft.Quantum.Intrinsic",
];

pub type Names = IndexMap<NodeId, Res>;

/// A resolution. This connects a usage of a name with the declaration of that name by uniquely
/// identifying the node that declared it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Res {
    /// A global item.
    Item(ItemId),
    /// A local variable.
    Local(NodeId),
    /// A type/functor parameter in the generics section of the parent callable decl.
    Param(ParamId),
    /// A primitive type.
    PrimTy(Prim),
    /// The unit type.
    UnitTy,
}

#[derive(Clone, Debug, Diagnostic, Error)]
pub(super) enum Error {
    #[error("`{name}` could refer to the item in `{first_open}` or `{second_open}`")]
    #[diagnostic(code("Qsc.Resolve.Ambiguous"))]
    Ambiguous {
        name: String,
        first_open: String,
        second_open: String,
        #[label("ambiguous name")]
        name_span: Span,
        #[label("found in this namespace")]
        first_open_span: Span,
        #[label("and also in this namespace")]
        second_open_span: Span,
    },

    #[error("duplicate declaration of `{0}` in namespace `{1}`")]
    #[diagnostic(code("Qsc.Resolve.Duplicate"))]
    Duplicate(String, String, #[label] Span),

    #[error("`{0}` not found")]
    #[diagnostic(code("Qsc.Resolve.NotFound"))]
    NotFound(String, #[label] Span),
}

struct Scope {
    kind: ScopeKind,
    opens: HashMap<Rc<str>, Vec<Open>>,
    tys: HashMap<Rc<str>, ItemId>,
    terms: HashMap<Rc<str>, ItemId>,
    vars: HashMap<Rc<str>, NodeId>,
    ty_vars: HashMap<Rc<str>, ParamId>,
}

impl Scope {
    fn new(kind: ScopeKind) -> Self {
        Self {
            kind,
            opens: HashMap::new(),
            tys: HashMap::new(),
            terms: HashMap::new(),
            vars: HashMap::new(),
            ty_vars: HashMap::new(),
        }
    }

    fn item(&self, kind: NameKind, name: &str) -> Option<&ItemId> {
        let items = match kind {
            NameKind::Ty => &self.tys,
            NameKind::Term => &self.terms,
        };
        items.get(name)
    }

    fn items(&self, kind: NameKind) -> Vec<&Rc<str>> {
        let items = match kind {
            NameKind::Ty => &self.tys,
            NameKind::Term => &self.terms,
        };
        items.keys().collect()
    }
}

struct GlobalScope {
    tys: HashMap<Rc<str>, HashMap<Rc<str>, Res>>,
    terms: HashMap<Rc<str>, HashMap<Rc<str>, Res>>,
    namespaces: HashSet<Rc<str>>,
}

impl GlobalScope {
    fn get(&self, kind: NameKind, namespace: &str, name: &str) -> Option<&Res> {
        let namespaces = match kind {
            NameKind::Ty => &self.tys,
            NameKind::Term => &self.terms,
        };
        namespaces.get(namespace).and_then(|items| items.get(name))
    }

    fn get_all(&self, kind: NameKind, namespace: &str) -> Vec<&Rc<str>> {
        let namespaces = match kind {
            NameKind::Ty => &self.tys,
            NameKind::Term => &self.terms,
        };
        namespaces
            .get(namespace)
            .and_then(|items| Some(items.keys().collect()))
            .unwrap_or(Vec::new())
    }
}

#[derive(Eq, PartialEq)]
enum ScopeKind {
    Namespace(Rc<str>),
    Callable,
    Block,
}

#[derive(Clone, Copy)]
enum NameKind {
    Ty,
    Term,
}

struct Open {
    namespace: Rc<str>,
    span: Span,
}

pub(super) struct Resolver {
    names: Names,
    globals: GlobalScope,
    scopes: Vec<Scope>,
    errors: Vec<Error>,
}

impl Resolver {
    pub(super) fn new(globals: GlobalTable) -> Self {
        Self {
            names: globals.names,
            globals: globals.scope,
            scopes: Vec::new(),
            errors: Vec::new(),
        }
    }

    pub(super) fn with_persistent_local_scope(globals: GlobalTable) -> Self {
        Self {
            names: globals.names,
            globals: globals.scope,
            scopes: vec![Scope::new(ScopeKind::Block)],
            errors: Vec::new(),
        }
    }

    pub(super) fn names(&self) -> &Names {
        &self.names
    }

    pub(super) fn drain_errors(&mut self) -> vec::Drain<Error> {
        self.errors.drain(..)
    }

    pub(super) fn with<'a>(&'a mut self, assigner: &'a mut Assigner) -> With<'a> {
        With {
            finder_mode: None,
            resolver: self,
            assigner,
            in_block: false,
        }
    }

    pub(super) fn with_term_finder<'a>(
        &'a mut self,
        assigner: &'a mut Assigner,
        offset: u32,
    ) -> With<'a> {
        With {
            finder_mode: Some(FindContext {
                offset,
                kind: Some(NameKind::Term),
                results: HashSet::new(),
            }),
            resolver: self,
            assigner,
            in_block: false,
        }
    }

    pub(super) fn with_type_finder<'a>(
        &'a mut self,
        assigner: &'a mut Assigner,
        offset: u32,
    ) -> With<'a> {
        With {
            finder_mode: Some(FindContext {
                offset,
                kind: Some(NameKind::Ty),
                results: HashSet::new(),
            }),
            resolver: self,
            assigner,
            in_block: false,
        }
    }

    pub(super) fn with_namespace_finder<'a>(
        &'a mut self,
        assigner: &'a mut Assigner,
        offset: u32,
    ) -> With<'a> {
        With {
            finder_mode: Some(FindContext {
                offset,
                kind: None,
                results: HashSet::new(),
            }),
            resolver: self,
            assigner,
            in_block: false,
        }
    }

    pub(super) fn into_names(self) -> (Names, Vec<Error>) {
        (self.names, self.errors)
    }

    fn resolve_ident(&mut self, kind: NameKind, name: &Ident) {
        let namespace = None;

        match resolve(kind, &self.globals, &self.scopes, name, &namespace) {
            Ok(id) => self.names.insert(name.id, id),
            Err(err) => self.errors.push(err),
        }
    }

    fn resolve_path(&mut self, kind: NameKind, path: &ast::Path) {
        let name = &path.name;
        let namespace = &path.namespace;

        match resolve(kind, &self.globals, &self.scopes, name, namespace) {
            Ok(id) => self.names.insert(path.id, id),
            Err(err) => self.errors.push(err),
        }
    }

    fn bind_pat(&mut self, pat: &ast::Pat) {
        match &*pat.kind {
            ast::PatKind::Bind(name, _) => {
                let scope = self.scopes.last_mut().expect("binding should have scope");
                self.names.insert(name.id, Res::Local(name.id));
                scope.vars.insert(Rc::clone(&name.name), name.id);
            }
            ast::PatKind::Discard(_) | ast::PatKind::Elided => {}
            ast::PatKind::Paren(pat) => self.bind_pat(pat),
            ast::PatKind::Tuple(pats) => pats.iter().for_each(|p| self.bind_pat(p)),
        }
    }

    fn bind_open(&mut self, name: &ast::Ident, alias: &Option<Box<ast::Ident>>) {
        let alias = alias.as_ref().map_or("".into(), |a| Rc::clone(&a.name));
        let scope = self.scopes.last_mut().expect("open item should have scope");
        if self.globals.namespaces.contains(&name.name) {
            scope.opens.entry(alias).or_default().push(Open {
                namespace: Rc::clone(&name.name),
                span: name.span,
            });
        } else {
            self.errors
                .push(Error::NotFound(name.name.to_string(), name.span));
        }
    }

    fn bind_local_item(&mut self, assigner: &mut Assigner, item: &ast::Item) {
        match &*item.kind {
            ast::ItemKind::Open(name, alias) => self.bind_open(name, alias),
            ast::ItemKind::Callable(decl) => {
                let id = intrapackage(assigner.next_item());
                self.names.insert(decl.name.id, Res::Item(id));
                let scope = self.scopes.last_mut().expect("binding should have scope");
                scope.terms.insert(Rc::clone(&decl.name.name), id);
            }
            ast::ItemKind::Ty(name, _) => {
                let id = intrapackage(assigner.next_item());
                self.names.insert(name.id, Res::Item(id));
                let scope = self.scopes.last_mut().expect("binding should have scope");
                scope.tys.insert(Rc::clone(&name.name), id);
                scope.terms.insert(Rc::clone(&name.name), id);
            }
            ast::ItemKind::Err => {}
        }
    }

    fn bind_type_parameters(&mut self, decl: &CallableDecl) {
        decl.generics.iter().enumerate().for_each(|(ix, ident)| {
            let scope = self
                .scopes
                .last_mut()
                .expect("type parameters should have scope");
            scope.ty_vars.insert(Rc::clone(&ident.name), ix.into());
            self.names.insert(ident.id, Res::Param(ix.into()));
        });
    }
}

pub(super) struct FindContext {
    offset: u32,
    kind: Option<NameKind>,
    pub results: HashSet<Rc<str>>,
}

pub(super) struct With<'a> {
    pub finder_mode: Option<FindContext>,
    resolver: &'a mut Resolver,
    assigner: &'a mut Assigner,
    in_block: bool,
}

impl With<'_> {
    fn with_scope(&mut self, span: Span, kind: ScopeKind, f: impl FnOnce(&mut Self)) {
        self.resolver.scopes.push(Scope::new(kind));
        f(self);

        if let Some(filter) = &mut self.finder_mode {
            if let Some(kind) = filter.kind {
                if span.lo <= filter.offset && span.hi > filter.offset {
                    // Of course we're going do to this and
                    // throw the result away a bunch of times
                    // if we're in nested scopes

                    // TODO: Not getting locals here, hmm
                    let terms = gather_names(
                        kind,
                        &self.resolver.globals,
                        &self.resolver.scopes,
                        &None, // TODO: need to try with namespaces for path
                    );
                    filter.results.clear();
                    filter.results.extend(terms);
                }
            }
        }

        self.resolver
            .scopes
            .pop()
            .expect("pushed scope should be the last element on the stack");
    }

    pub fn find_results(&self) -> (HashSet<Rc<str>>, HashSet<Rc<str>>) {
        // names and namespaces
        (
            self.finder_mode
                .as_ref()
                .expect("don't call find_results if you're not in finder mode")
                .results
                .clone(),
            self.resolver.globals.namespaces.clone(),
        )
    }

    fn with_pat(&mut self, span: Span, kind: ScopeKind, pat: &ast::Pat, f: impl FnOnce(&mut Self)) {
        self.with_scope(span, kind, |visitor| {
            visitor.resolver.bind_pat(pat);
            f(visitor);
        });
    }
}

impl AstVisitor<'_> for With<'_> {
    fn visit_namespace(&mut self, namespace: &ast::Namespace) {
        if !self.resolver.names.contains_key(namespace.name.id) {
            let id = self.assigner.next_item();
            self.resolver
                .names
                .insert(namespace.name.id, Res::Item(intrapackage(id)));
            self.resolver
                .globals
                .namespaces
                .insert(Rc::clone(&namespace.name.name));

            for item in namespace.items.iter() {
                match bind_global_item(
                    &mut self.resolver.names,
                    &mut self.resolver.globals,
                    &namespace.name.name,
                    || intrapackage(self.assigner.next_item()),
                    item,
                ) {
                    Ok(()) => {}
                    Err(error) => self.resolver.errors.push(error),
                }
            }
        }

        let kind = ScopeKind::Namespace(Rc::clone(&namespace.name.name));
        self.with_scope(namespace.span, kind, |visitor| {
            for item in namespace.items.iter() {
                if let ast::ItemKind::Open(name, alias) = &*item.kind {
                    visitor.resolver.bind_open(name, alias);
                }
            }

            ast_visit::walk_namespace(visitor, namespace);
        });
    }

    fn visit_callable_decl(&mut self, decl: &ast::CallableDecl) {
        self.with_scope(decl.span, ScopeKind::Callable, |visitor| {
            visitor.resolver.bind_type_parameters(decl);
            visitor.resolver.bind_pat(&decl.input);
            ast_visit::walk_callable_decl(visitor, decl);
        });
    }

    fn visit_spec_decl(&mut self, decl: &ast::SpecDecl) {
        if let ast::SpecBody::Impl(input, block) = &decl.body {
            self.with_pat(decl.span, ScopeKind::Block, input, |visitor| {
                visitor.visit_block(block);
            });
        } else {
            ast_visit::walk_spec_decl(self, decl);
        }
    }

    fn visit_ty(&mut self, ty: &ast::Ty) {
        match &*ty.kind {
            ast::TyKind::Path(path) => {
                self.resolver.resolve_path(NameKind::Ty, path);
            }
            ast::TyKind::Param(ident) => {
                self.resolver.resolve_ident(NameKind::Ty, ident);
            }
            _ => ast_visit::walk_ty(self, ty),
        }
    }

    fn visit_block(&mut self, block: &ast::Block) {
        let prev = self.in_block;
        self.in_block = true;
        self.with_scope(block.span, ScopeKind::Block, |visitor| {
            for stmt in block.stmts.iter() {
                if let ast::StmtKind::Item(item) = &*stmt.kind {
                    visitor.resolver.bind_local_item(visitor.assigner, item);
                }
            }

            ast_visit::walk_block(visitor, block);
        });
        self.in_block = prev;
    }

    fn visit_stmt(&mut self, stmt: &ast::Stmt) {
        match &*stmt.kind {
            ast::StmtKind::Item(item) => {
                if !self.in_block {
                    self.resolver.bind_local_item(self.assigner, item);
                }
                self.visit_item(item);
            }
            ast::StmtKind::Local(_, pat, _) => {
                ast_visit::walk_stmt(self, stmt);
                self.resolver.bind_pat(pat);
            }
            ast::StmtKind::Qubit(_, pat, init, block) => {
                ast_visit::walk_qubit_init(self, init);
                self.resolver.bind_pat(pat);
                if let Some(block) = block {
                    self.visit_block(block);
                }
            }
            ast::StmtKind::Empty
            | ast::StmtKind::Expr(_)
            | ast::StmtKind::Semi(_)
            | ast::StmtKind::Err => {
                ast_visit::walk_stmt(self, stmt);
            }
        }
    }

    fn visit_expr(&mut self, expr: &ast::Expr) {
        match &*expr.kind {
            ast::ExprKind::For(pat, iter, block) => {
                self.visit_expr(iter);
                self.with_pat(expr.span, ScopeKind::Block, pat, |visitor| {
                    visitor.visit_block(block)
                });
            }
            ast::ExprKind::Lambda(_, input, output) => {
                self.with_pat(expr.span, ScopeKind::Block, input, |visitor| {
                    visitor.visit_expr(output);
                });
            }
            ast::ExprKind::Path(path) => self.resolver.resolve_path(NameKind::Term, path),
            ast::ExprKind::TernOp(ast::TernOp::Update, container, index, replace) => {
                self.visit_expr(container);
                if !is_field_update(&self.resolver.globals, &self.resolver.scopes, index) {
                    self.visit_expr(index);
                }
                self.visit_expr(replace);
            }
            _ => ast_visit::walk_expr(self, expr),
        }
    }
}

pub(super) struct GlobalTable {
    names: Names,
    scope: GlobalScope,
}

impl GlobalTable {
    pub(super) fn new() -> Self {
        let tys = HashMap::from([(
            "Microsoft.Quantum.Core".into(),
            HashMap::from([
                ("BigInt".into(), Res::PrimTy(Prim::BigInt)),
                ("Bool".into(), Res::PrimTy(Prim::Bool)),
                ("Double".into(), Res::PrimTy(Prim::Double)),
                ("Int".into(), Res::PrimTy(Prim::Int)),
                ("Pauli".into(), Res::PrimTy(Prim::Pauli)),
                ("Qubit".into(), Res::PrimTy(Prim::Qubit)),
                ("Range".into(), Res::PrimTy(Prim::Range)),
                ("Result".into(), Res::PrimTy(Prim::Result)),
                ("String".into(), Res::PrimTy(Prim::String)),
                ("Unit".into(), Res::UnitTy),
            ]),
        )]);

        Self {
            names: IndexMap::new(),
            scope: GlobalScope {
                tys,
                terms: HashMap::new(),
                namespaces: HashSet::new(),
            },
        }
    }

    pub(super) fn add_local_package(
        &mut self,
        assigner: &mut Assigner,
        package: &ast::Package,
    ) -> Vec<Error> {
        let mut errors = Vec::new();
        for namespace in package.namespaces.iter() {
            self.names.insert(
                namespace.name.id,
                Res::Item(intrapackage(assigner.next_item())),
            );
            self.scope
                .namespaces
                .insert(Rc::clone(&namespace.name.name));

            for item in namespace.items.iter() {
                match bind_global_item(
                    &mut self.names,
                    &mut self.scope,
                    &namespace.name.name,
                    || intrapackage(assigner.next_item()),
                    item,
                ) {
                    Ok(()) => {}
                    Err(error) => errors.push(error),
                }
            }
        }
        errors
    }

    pub(super) fn add_external_package(&mut self, id: PackageId, package: &hir::Package) {
        for global in global::iter_package(Some(id), package)
            .filter(|global| global.visibility == hir::Visibility::Public)
        {
            match global.kind {
                global::Kind::Ty(ty) => {
                    self.scope
                        .tys
                        .entry(global.namespace)
                        .or_default()
                        .insert(global.name, Res::Item(ty.id));
                }
                global::Kind::Term(term) => {
                    self.scope
                        .terms
                        .entry(global.namespace)
                        .or_default()
                        .insert(global.name, Res::Item(term.id));
                }
                global::Kind::Namespace => {
                    self.scope.namespaces.insert(global.name);
                }
            }
        }
    }
}

/// Tries to extract a field name from an expression in cases where it is syntactically ambiguous
/// whether the expression is a field name or a variable name. This applies to the index operand in
/// a ternary update operator.
pub(super) fn extract_field_name<'a>(names: &Names, expr: &'a ast::Expr) -> Option<&'a Rc<str>> {
    // Follow the same reasoning as `is_field_update`.
    match &*expr.kind {
        ast::ExprKind::Path(path)
            if path.namespace.is_none() && !matches!(names.get(path.id), Some(Res::Local(_))) =>
        {
            Some(&path.name.name)
        }
        _ => None,
    }
}

fn is_field_update(globals: &GlobalScope, scopes: &[Scope], index: &ast::Expr) -> bool {
    // Disambiguate the update operator by looking at the index expression. If it's an
    // unqualified path that doesn't resolve to a local, assume that it's meant to be a field name.
    match &*index.kind {
        ast::ExprKind::Path(path) if path.namespace.is_none() => !matches!(
            {
                let name = &path.name;
                let namespace = &path.namespace;
                resolve(NameKind::Term, globals, scopes, name, namespace)
            },
            Ok(Res::Local(_))
        ),
        _ => false,
    }
}

fn bind_global_item(
    names: &mut Names,
    scope: &mut GlobalScope,
    namespace: &Rc<str>,
    next_id: impl FnOnce() -> ItemId,
    item: &ast::Item,
) -> Result<(), Error> {
    match &*item.kind {
        ast::ItemKind::Callable(decl) => {
            let res = Res::Item(next_id());
            names.insert(decl.name.id, res);
            match scope
                .terms
                .entry(Rc::clone(namespace))
                .or_default()
                .entry(Rc::clone(&decl.name.name))
            {
                Entry::Occupied(_) => Err(Error::Duplicate(
                    decl.name.name.to_string(),
                    namespace.to_string(),
                    decl.name.span,
                )),
                Entry::Vacant(entry) => {
                    entry.insert(res);
                    Ok(())
                }
            }
        }
        ast::ItemKind::Ty(name, _) => {
            let res = Res::Item(next_id());
            names.insert(name.id, res);
            match (
                scope
                    .terms
                    .entry(Rc::clone(namespace))
                    .or_default()
                    .entry(Rc::clone(&name.name)),
                scope
                    .tys
                    .entry(Rc::clone(namespace))
                    .or_default()
                    .entry(Rc::clone(&name.name)),
            ) {
                (Entry::Occupied(_), _) | (_, Entry::Occupied(_)) => Err(Error::Duplicate(
                    name.name.to_string(),
                    namespace.to_string(),
                    name.span,
                )),
                (Entry::Vacant(term_entry), Entry::Vacant(ty_entry)) => {
                    term_entry.insert(res);
                    ty_entry.insert(res);
                    Ok(())
                }
            }
        }
        ast::ItemKind::Err | ast::ItemKind::Open(..) => Ok(()),
    }
}

fn gather_names(
    kind: NameKind,
    globals: &GlobalScope,
    locals: &[Scope],
    namespace: &Option<Box<Ident>>,
) -> Vec<Rc<str>> {
    let mut vars = true;
    let mut names = Vec::new();
    let namespace = namespace.as_ref().map_or("", |i| &i.name);
    for scope in locals.iter().rev() {
        if namespace.is_empty() {
            names.extend(gather_scope_locals(kind, globals, scope, vars));
        }

        if let Some(namespaces) = scope.opens.get(namespace) {
            names.extend(gather_explicit_opens(kind, globals, namespaces));
        }

        if scope.kind == ScopeKind::Callable {
            // Since local callables are not closures, hide local variables in parent scopes.
            vars = false;
        }
    }

    if namespace.is_empty() {
        names.extend(gather_implicit_opens(kind, globals, PRELUDE));
    }

    names.extend(
        globals
            .get_all(kind, namespace)
            .into_iter()
            .map(|s| s.clone()),
    );
    names
}

fn resolve(
    kind: NameKind,
    globals: &GlobalScope,
    locals: &[Scope],
    name: &Ident,
    namespace: &Option<Box<Ident>>,
) -> Result<Res, Error> {
    let mut candidates = HashMap::new();
    let mut vars = true;
    let name_str = &(*name.name);
    let namespace = namespace.as_ref().map_or("", |i| &i.name);
    for scope in locals.iter().rev() {
        if namespace.is_empty() {
            if let Some(res) = resolve_scope_locals(kind, globals, scope, vars, name_str) {
                // Local declarations shadow everything.
                return Ok(res);
            }
        }

        if let Some(namespaces) = scope.opens.get(namespace) {
            candidates = resolve_explicit_opens(kind, globals, namespaces, name_str);
            if !candidates.is_empty() {
                // Explicit opens shadow prelude and unopened globals.
                break;
            }
        }

        if scope.kind == ScopeKind::Callable {
            // Since local callables are not closures, hide local variables in parent scopes.
            vars = false;
        }
    }

    if candidates.is_empty() && namespace.is_empty() {
        // Prelude shadows unopened globals.
        let candidates = resolve_implicit_opens(kind, globals, PRELUDE, name_str);
        assert!(candidates.len() <= 1, "ambiguity in prelude resolution");
        if let Some(res) = single(candidates) {
            return Ok(res);
        }
    }

    if candidates.is_empty() {
        if let Some(&res) = globals.get(kind, namespace, name_str) {
            // An unopened global is the last resort.
            return Ok(res);
        }
    }

    if candidates.len() > 1 {
        let mut opens: Vec<_> = candidates.into_values().collect();
        opens.sort_unstable_by_key(|open| open.span);
        Err(Error::Ambiguous {
            name: name_str.to_string(),
            first_open: opens[0].namespace.to_string(),
            second_open: opens[1].namespace.to_string(),
            name_span: name.span,
            first_open_span: opens[0].span,
            second_open_span: opens[1].span,
        })
    } else {
        single(candidates.into_keys())
            .ok_or_else(|| Error::NotFound(name_str.to_string(), name.span))
    }
}

/// Implements shadowing rules within a single scope.
/// A local variable always wins out against an item with the same name, if they're declared in
/// the same scope. It is implemented in a way that resembles Rust:
/// ```rust
/// let foo = || 1;
/// fn foo() -> i32 { 2 }
/// dbg!(foo()); // 1, not 2
/// ```
fn resolve_scope_locals(
    kind: NameKind,
    globals: &GlobalScope,
    scope: &Scope,
    vars: bool,
    name: &str,
) -> Option<Res> {
    if vars {
        match kind {
            NameKind::Term => {
                if let Some(&id) = scope.vars.get(name) {
                    return Some(Res::Local(id));
                }
            }
            NameKind::Ty => {
                if let Some(&id) = scope.ty_vars.get(name) {
                    return Some(Res::Param(id));
                }
            }
        }
    }

    if let Some(&id) = scope.item(kind, name) {
        return Some(Res::Item(id));
    }

    if let ScopeKind::Namespace(namespace) = &scope.kind {
        if let Some(&res) = globals.get(kind, namespace, name) {
            return Some(res);
        }
    }

    None
}

fn gather_scope_locals(
    kind: NameKind,
    globals: &GlobalScope,
    scope: &Scope,
    vars: bool,
) -> Vec<Rc<str>> {
    let mut names = Vec::new();
    if vars {
        match kind {
            NameKind::Term => {
                names.extend(scope.vars.iter().map(|id| id.0));
            }
            NameKind::Ty => {
                names.extend(scope.ty_vars.iter().map(|id| id.0));
            }
        }
    }

    names.extend(scope.items(kind).iter());

    if let ScopeKind::Namespace(namespace) = &scope.kind {
        names.extend(globals.get_all(kind, namespace).iter());
    }

    // Not sure why
    names.into_iter().map(|s| s.clone()).collect()
}

fn gather_implicit_opens(
    kind: NameKind,
    globals: &GlobalScope,
    namespaces: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<Rc<str>> {
    let mut names = Vec::new();
    for namespace in namespaces {
        names.extend(globals.get_all(kind, namespace.as_ref()));
    }
    names.into_iter().map(|s| s.clone()).collect()
}

fn resolve_implicit_opens(
    kind: NameKind,
    globals: &GlobalScope,
    namespaces: impl IntoIterator<Item = impl AsRef<str>>,
    name: &str,
) -> HashSet<Res> {
    let mut candidates = HashSet::new();
    for namespace in namespaces {
        let namespace = namespace.as_ref();
        if let Some(&res) = globals.get(kind, namespace, name) {
            candidates.insert(res);
        }
    }
    candidates
}

fn gather_explicit_opens<'a>(
    kind: NameKind,
    globals: &GlobalScope,
    opens: impl IntoIterator<Item = &'a Open>,
) -> Vec<Rc<str>> {
    let mut names = Vec::new();
    for open in opens {
        names.extend(globals.get_all(kind, &open.namespace));
    }
    names.into_iter().map(|s| s.clone()).collect()
}

fn resolve_explicit_opens<'a>(
    kind: NameKind,
    globals: &GlobalScope,
    opens: impl IntoIterator<Item = &'a Open>,
    name: &str,
) -> HashMap<Res, &'a Open> {
    let mut candidates = HashMap::new();
    for open in opens {
        if let Some(&res) = globals.get(kind, &open.namespace, name) {
            candidates.insert(res, open);
        }
    }
    candidates
}

fn intrapackage(item: LocalItemId) -> ItemId {
    ItemId {
        package: None,
        item,
    }
}

fn single<T>(xs: impl IntoIterator<Item = T>) -> Option<T> {
    let mut xs = xs.into_iter();
    let x = xs.next();
    match xs.next() {
        None => x,
        Some(_) => None,
    }
}
