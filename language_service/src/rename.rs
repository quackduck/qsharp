// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::compilation::{Compilation, Lookup};
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::protocol::{self, Location};
use crate::qsc_utils::protocol_span;
use crate::references::{find_field_locations, find_item_locations, find_local_locations};
use qsc::ast::visit::Visitor;
use qsc::{ast, hir, resolve, Span};

pub(crate) fn prepare_rename(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Option<(protocol::Span, String)> {
    let (ast, offset) = compilation.resolve_offset(source_name, offset);

    let mut prepare_rename = Rename::new(compilation, true);
    let mut locator = Locator::new(&mut prepare_rename, offset, compilation);
    locator.visit_package(&ast.package);
    prepare_rename
        .prepare
        .map(|p| (protocol_span(p.0, &compilation.user_unit().sources), p.1))
}

pub(crate) fn get_rename(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Vec<Location> {
    let (ast, offset) = compilation.resolve_offset(source_name, offset);

    let mut rename = Rename::new(compilation, false);
    let mut locator = Locator::new(&mut rename, offset, compilation);
    locator.visit_package(&ast.package);
    rename.locations
}

struct Rename<'a> {
    compilation: &'a Compilation,
    locations: Vec<Location>,
    is_prepare: bool,
    prepare: Option<(Span, String)>,
}

impl<'a> Rename<'a> {
    fn new(compilation: &'a Compilation, is_prepare: bool) -> Self {
        Self {
            compilation,
            locations: vec![],
            is_prepare,
            prepare: None,
        }
    }

    fn get_spans_for_item_rename(&mut self, item_id: &hir::ItemId, ast_name: &ast::Ident) {
        let package_id = item_id.package.expect("package id should be resolved");
        // Only rename items that are part of the user package
        if package_id == self.compilation.user {
            if self.is_prepare {
                self.prepare = Some((ast_name.span, ast_name.name.to_string()));
            } else {
                self.locations = find_item_locations(item_id, self.compilation, true);
            }
        }
    }

    fn get_spans_for_field_rename(&mut self, item_id: &hir::ItemId, ast_name: &ast::Ident) {
        let package_id = item_id.package.expect("package id should be resolved");
        // Only rename items that are part of the user package
        if package_id == self.compilation.user {
            if self.is_prepare {
                self.prepare = Some((ast_name.span, ast_name.name.to_string()));
            } else {
                self.locations =
                    find_field_locations(item_id, ast_name.name.clone(), self.compilation, true);
            }
        }
    }

    fn get_spans_for_local_rename(
        &mut self,
        node_id: ast::NodeId,
        ast_name: &ast::Ident,
        current_callable: &ast::CallableDecl,
    ) {
        if self.is_prepare {
            self.prepare = Some((ast_name.span, ast_name.name.to_string()));
        } else {
            self.locations =
                find_local_locations(node_id, current_callable, self.compilation, true);
        }
    }
}

impl<'a> Handler<'a> for Rename<'a> {
    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        if let Some(resolve::Res::Item(item_id, _)) = self.compilation.get_res(name.id) {
            self.get_spans_for_item_rename(&resolve_package(self.compilation.user, item_id), name);
        }
    }

    fn at_callable_ref(
        &mut self,
        path: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Item,
        _: &'a hir::Package,
        _: &'a hir::CallableDecl,
    ) {
        self.get_spans_for_item_rename(item_id, &path.name);
    }

    fn at_new_type_def(&mut self, type_name: &'a ast::Ident, _: &'a ast::TyDef) {
        if let Some(resolve::Res::Item(item_id, _)) = self.compilation.get_res(type_name.id) {
            self.get_spans_for_item_rename(
                &resolve_package(self.compilation.user, item_id),
                type_name,
            );
        }
    }

    fn at_new_type_ref(
        &mut self,
        path: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Package,
        _: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.get_spans_for_item_rename(item_id, &path.name);
    }

    fn at_field_def(
        &mut self,
        context: &LocatorContext<'a>,
        field_name: &'a ast::Ident,
        _: &'a ast::Ty,
    ) {
        if let Some(item_id) = context.current_udt_id {
            self.get_spans_for_field_rename(
                &resolve_package(self.compilation.user, item_id),
                field_name,
            );
        }
    }

    fn at_field_ref(
        &mut self,
        field_ref: &'a ast::Ident,
        _: &'a ast::NodeId,
        item_id: &'_ hir::ItemId,
        _: &'a hir::ty::UdtField,
    ) {
        self.get_spans_for_field_rename(item_id, field_ref);
    }

    fn at_local_def(
        &mut self,
        context: &LocatorContext<'a>,
        ident: &'a ast::Ident,
        _: &'a ast::Pat,
    ) {
        if let Some(curr) = context.current_callable {
            self.get_spans_for_local_rename(ident.id, ident, curr);
        }
    }

    fn at_local_ref(
        &mut self,
        context: &LocatorContext<'a>,
        path: &'a ast::Path,
        node_id: &'a ast::NodeId,
        _: &'a ast::Ident,
    ) {
        if let Some(curr) = context.current_callable {
            self.get_spans_for_local_rename(*node_id, &path.name, curr);
        }
    }
}

fn resolve_package(local_package_id: hir::PackageId, item_id: &hir::ItemId) -> hir::ItemId {
    hir::ItemId {
        item: item_id.item,
        package: item_id.package.or(Some(local_package_id)),
    }
}
