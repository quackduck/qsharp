// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::qsc_utils::span_contains;
use qsc::{
    ast::{
        self,
        visit::{walk_expr, walk_namespace, walk_pat, walk_ty_def, Visitor},
        CallableDecl, Expr, Ident, Package, Pat, Path as AstPath, Ty, TyDef,
    },
    Span,
};

#[cfg(test)]
mod tests;

pub(crate) struct Node<'a> {
    pub span: Span,
    pub kind: NodeKind<'a>,
}

pub(crate) enum NodeKind<'a> {
    CallableDeclName(&'a CallableDecl, &'a str, &'a str),
    TyDefName(&'a Ident, &'a TyDef),
    TyDefFieldName(&'a Ident, &'a Ty),
    PatBindName(PatBindName<'a>),
    ExprField(ExprField<'a>),
    Path(Path<'a>),
}

pub(crate) struct Path<'a> {
    pub node: &'a AstPath,
    pub current_item_doc: &'a str,
    pub current_callable: Option<&'a ast::CallableDecl>,
    pub lambda_params: Vec<&'a ast::Pat>,
}

/// The name node in a binding
pub(crate) struct PatBindName<'a> {
    pub node: &'a Ident,
    pub pat: &'a Pat,
    pub current_item_doc: &'a str,
    pub current_callable: Option<&'a ast::CallableDecl>,
    pub in_params: bool,
    pub in_lambda_params: bool,
}

/// The field node in a field accessor
pub(crate) struct ExprField<'a> {
    pub node: &'a Expr,
    pub record: &'a Expr,
    pub field: &'a Ident,
}

pub(crate) fn node_by_offset(offset: u32, package: &Package) -> Option<Node> {
    let mut visitor = ByOffset {
        offset,
        current_namespace: "",
        current_item_doc: "",
        current_callable: Option::default(),
        in_params: false,
        in_lambda_params: false,
        lambda_params: Vec::new(),
        node: Option::default(),
    };
    visitor.visit_package(package);
    visitor.into_node()
}

struct ByOffset<'a> {
    offset: u32,
    current_namespace: &'a str,
    current_item_doc: &'a str,
    current_callable: Option<&'a ast::CallableDecl>,
    in_params: bool,
    in_lambda_params: bool,
    lambda_params: Vec<&'a ast::Pat>,
    node: Option<Node<'a>>,
}

impl<'a> ByOffset<'a> {
    pub fn into_node(self) -> Option<Node<'a>> {
        self.node
    }

    fn set_node_kind(&mut self, span: Span, kind: NodeKind<'a>) {
        self.node = Some(Node { span, kind });
    }
}

impl<'a> Visitor<'a> for ByOffset<'a> {
    fn visit_namespace(&mut self, namespace: &'a ast::Namespace) {
        if span_contains(namespace.span, self.offset) {
            self.current_namespace = &namespace.name.name;
            walk_namespace(self, namespace);
        }
    }

    // Handles callable and UDT definitions
    fn visit_item(&mut self, item: &'a ast::Item) {
        let context = self.current_item_doc;
        self.current_item_doc = &item.doc;
        if span_contains(item.span, self.offset) {
            match &*item.kind {
                ast::ItemKind::Callable(decl) => {
                    if span_contains(decl.name.span, self.offset) {
                        self.set_node_kind(
                            decl.name.span,
                            NodeKind::CallableDeclName(decl, &item.doc, self.current_namespace),
                        );
                    } else if span_contains(decl.span, self.offset) {
                        let context = self.current_callable;
                        self.current_callable = Some(decl);

                        // walk callable decl
                        decl.generics.iter().for_each(|p| self.visit_ident(p));
                        self.in_params = true;
                        self.visit_pat(&decl.input);
                        self.in_params = false;
                        self.visit_ty(&decl.output);
                        match &*decl.body {
                            ast::CallableBody::Block(block) => self.visit_block(block),
                            ast::CallableBody::Specs(specs) => {
                                specs.iter().for_each(|s| self.visit_spec_decl(s));
                            }
                        }

                        self.current_callable = context;
                    }
                    // Note: the `item.span` can cover things like doc
                    // comment, attributes, and visibility keywords, which aren't
                    // things we want to have hover logic for, while the `decl.span` is
                    // specific to the contents of the callable decl, which we do want
                    // hover logic for. If the `if` or `else if` above is not met, then
                    // the user is hovering over one of these non-decl parts of the item,
                    // and we want to do nothing.
                }
                ast::ItemKind::Ty(ident, def) => {
                    if span_contains(ident.span, self.offset) {
                        self.set_node_kind(ident.span, NodeKind::TyDefName(ident, def));
                    } else {
                        self.visit_ty_def(def);
                    }
                }
                _ => {}
            }
        }
        self.current_item_doc = context;
    }

    // Handles UDT field definitions
    fn visit_ty_def(&mut self, def: &'a ast::TyDef) {
        if span_contains(def.span, self.offset) {
            if let ast::TyDefKind::Field(ident, ty) = &*def.kind {
                if let Some(ident) = ident {
                    if span_contains(ident.span, self.offset) {
                        self.set_node_kind(ident.span, NodeKind::TyDefFieldName(ident, ty));
                    } else {
                        self.visit_ty(ty);
                    }
                } else {
                    self.visit_ty(ty);
                }
            } else {
                walk_ty_def(self, def);
            }
        }
    }

    fn visit_spec_decl(&mut self, decl: &'a ast::SpecDecl) {
        // Walk Spec Decl
        match &decl.body {
            ast::SpecBody::Gen(_) => {}
            ast::SpecBody::Impl(pat, block) => {
                self.in_params = true;
                self.visit_pat(pat);
                self.in_params = false;
                self.visit_block(block);
            }
        }
    }

    // Handles UDT field references
    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        if span_contains(expr.span, self.offset) {
            match &*expr.kind {
                ast::ExprKind::Field(record, field) if span_contains(field.span, self.offset) => {
                    self.set_node_kind(
                        field.span,
                        NodeKind::ExprField(ExprField {
                            node: expr,
                            record,
                            field,
                        }),
                    );
                }
                ast::ExprKind::Lambda(_, pat, expr) => {
                    self.in_lambda_params = true;
                    self.visit_pat(pat);
                    self.in_lambda_params = false;
                    self.lambda_params.push(pat);
                    self.visit_expr(expr);
                    self.lambda_params.pop();
                }
                _ => walk_expr(self, expr),
            }
        }
    }

    // Handles local variable definitions
    fn visit_pat(&mut self, pat: &'a ast::Pat) {
        if span_contains(pat.span, self.offset) {
            match &*pat.kind {
                ast::PatKind::Bind(ident, anno) => {
                    if span_contains(ident.span, self.offset) {
                        self.set_node_kind(
                            ident.span,
                            NodeKind::PatBindName(PatBindName {
                                node: ident,
                                pat,
                                current_item_doc: self.current_item_doc,
                                current_callable: self.current_callable,
                                in_params: self.in_params,
                                in_lambda_params: self.in_lambda_params,
                            }),
                        );
                    } else if let Some(ty) = anno {
                        self.visit_ty(ty);
                    }
                }
                _ => walk_pat(self, pat),
            }
        }
    }

    // Handles local variable, UDT, and callable references
    fn visit_path(&mut self, path: &'a ast::Path) {
        if span_contains(path.span, self.offset) {
            self.set_node_kind(
                path.span,
                NodeKind::Path(Path {
                    node: path,
                    current_item_doc: self.current_item_doc,
                    current_callable: self.current_callable,
                    lambda_params: self.lambda_params.clone(),
                }),
            );
        }
    }
}
