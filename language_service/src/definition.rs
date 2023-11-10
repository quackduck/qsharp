// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::compilation::Compilation;
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::protocol::Location;
use crate::qsc_utils::protocol_location;
use qsc::ast::visit::Visitor;
use qsc::{ast, hir};

pub(crate) fn get_definition(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Option<Location> {
    let (ast, offset) = compilation.resolve_offset(source_name, offset);

    let mut definition_finder = DefinitionFinder {
        compilation,
        definition: None,
    };

    let mut locator = Locator::new(&mut definition_finder, offset, compilation);
    locator.visit_package(&ast.package);

    definition_finder.definition
}

struct DefinitionFinder<'a> {
    compilation: &'a Compilation,
    definition: Option<Location>,
}

impl<'a> Handler<'a> for DefinitionFinder<'a> {
    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        self.definition = Some(protocol_location(
            self.compilation,
            name.span,
            self.compilation.user,
        ));
    }

    fn at_callable_ref(
        &mut self,
        _: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Item,
        _: &'a hir::Package,
        decl: &'a hir::CallableDecl,
    ) {
        self.definition = Some(protocol_location(
            self.compilation,
            decl.name.span,
            item_id.package.expect("package id should be resolved"),
        ));
    }

    fn at_new_type_def(&mut self, type_name: &'a ast::Ident, _: &'a ast::TyDef) {
        self.definition = Some(protocol_location(
            self.compilation,
            type_name.span,
            self.compilation.user,
        ));
    }

    fn at_new_type_ref(
        &mut self,
        _: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Package,
        type_name: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.definition = Some(protocol_location(
            self.compilation,
            type_name.span,
            item_id.package.expect("package id should be resolved"),
        ));
    }

    fn at_field_def(&mut self, _: &LocatorContext<'a>, field_name: &'a ast::Ident, _: &'a ast::Ty) {
        self.definition = Some(protocol_location(
            self.compilation,
            field_name.span,
            self.compilation.user,
        ));
    }

    fn at_field_ref(
        &mut self,
        _: &'a ast::Ident,
        _: &'a ast::NodeId,
        item_id: &'_ hir::ItemId,
        field_def: &'a hir::ty::UdtField,
    ) {
        let span = field_def
            .name_span
            .expect("field found via name should have a name");
        self.definition = Some(protocol_location(
            self.compilation,
            span,
            item_id.package.expect("package id should be resolved"),
        ));
    }

    fn at_local_def(&mut self, _: &LocatorContext<'a>, ident: &'a ast::Ident, _: &'a ast::Pat) {
        self.definition = Some(protocol_location(
            self.compilation,
            ident.span,
            self.compilation.user,
        ));
    }

    fn at_local_ref(
        &mut self,
        _: &LocatorContext<'a>,
        _: &'a ast::Path,
        _: &'a ast::NodeId,
        ident: &'a ast::Ident,
    ) {
        self.definition = Some(protocol_location(
            self.compilation,
            ident.span,
            self.compilation.user,
        ));
    }
}
