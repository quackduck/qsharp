// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::compilation::Compilation;
use crate::name_locator::{Handler, Locator, LocatorContext};
use crate::protocol::Definition;
use crate::qsc_utils::QSHARP_LIBRARY_URI_SCHEME;
use qsc::ast::visit::Visitor;
use qsc::hir::PackageId;
use qsc::{ast, hir};

pub(crate) fn get_definition(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Option<Definition> {
    let (ast, offset) = compilation.resolve_offset(source_name, offset);

    let mut definition_finder = DefinitionFinder {
        compilation,
        definition: None,
    };

    let mut locator = Locator::new(&mut definition_finder, offset, compilation);
    locator.visit_package(&ast.package);

    definition_finder
        .definition
        .map(|(name, offset)| Definition {
            source: name,
            offset,
        })
}

struct DefinitionFinder<'a> {
    compilation: &'a Compilation,
    definition: Option<(String, u32)>,
}

impl DefinitionFinder<'_> {
    fn set_definition_from_position(&mut self, lo: u32, package_id: PackageId) {
        let source = self
            .compilation
            .package_store
            .get(package_id)
            .expect("package id must exist in store")
            .sources
            .find_by_offset(lo)
            .expect("source should exist for offset");
        // Note: Having a package_id means the position references a foreign package.
        // Currently the only supported foreign packages are our library packages,
        // URI's to which need to include our custom library scheme.
        let source_name = if package_id == self.compilation.user {
            source.name.to_string()
        } else {
            format!("{}:{}", QSHARP_LIBRARY_URI_SCHEME, source.name)
        };

        self.definition = Some((source_name, lo - source.offset));
    }
}

impl<'a> Handler<'a> for DefinitionFinder<'a> {
    fn at_callable_def(
        &mut self,
        _: &LocatorContext<'a>,
        name: &'a ast::Ident,
        _: &'a ast::CallableDecl,
    ) {
        self.set_definition_from_position(name.span.lo, self.compilation.user);
    }

    fn at_callable_ref(
        &mut self,
        _: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Item,
        _: &'a hir::Package,
        decl: &'a hir::CallableDecl,
    ) {
        self.set_definition_from_position(
            decl.name.span.lo,
            item_id.package.expect("package id should be resolved"),
        );
    }

    fn at_new_type_def(&mut self, type_name: &'a ast::Ident, _: &'a ast::TyDef) {
        self.set_definition_from_position(type_name.span.lo, self.compilation.user);
    }

    fn at_new_type_ref(
        &mut self,
        _: &'a ast::Path,
        item_id: &'_ hir::ItemId,
        _: &'a hir::Package,
        type_name: &'a hir::Ident,
        _: &'a hir::ty::Udt,
    ) {
        self.set_definition_from_position(
            type_name.span.lo,
            item_id.package.expect("package id should be resolved"),
        );
    }

    fn at_field_def(&mut self, _: &LocatorContext<'a>, field_name: &'a ast::Ident, _: &'a ast::Ty) {
        self.set_definition_from_position(field_name.span.lo, self.compilation.user);
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
        self.set_definition_from_position(
            span.lo,
            item_id.package.expect("package id should be resolved"),
        );
    }

    fn at_local_def(&mut self, _: &LocatorContext<'a>, ident: &'a ast::Ident, _: &'a ast::Pat) {
        self.set_definition_from_position(ident.span.lo, self.compilation.user);
    }

    fn at_local_ref(
        &mut self,
        _: &LocatorContext<'a>,
        _: &'a ast::Path,
        _: &'a ast::NodeId,
        ident: &'a ast::Ident,
    ) {
        self.set_definition_from_position(ident.span.lo, self.compilation.user);
    }
}
