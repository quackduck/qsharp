// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use qsc::hir::{Package, PackageId};
use qsc::resolve::Names;
use qsc::typeck::Table;
use qsc::{ast, CompletionConstraint, Span};
use qsc::{
    compile::{self, Error},
    PackageStore, SourceMap,
};

/// Represents an immutable compilation state that can be used
/// to implement language service features.
pub(crate) struct Compilation {
    pub package_store: PackageStore,
    pub std_package_id: PackageId,
    pub ast_package: ast::Package,
    #[allow(dead_code)]
    pub names: Names,
    #[allow(dead_code)]
    pub tys: Table,
    pub package: Package,
    pub source_map: SourceMap,
    pub errors: Vec<Error>,
}

pub(crate) fn compile_document(source_name: &str, source_contents: &str) -> Compilation {
    let mut package_store = PackageStore::new(compile::core());
    let std_package_id = package_store.insert(compile::std(&package_store));

    // Source map only contains the current document.
    let source_map = SourceMap::new([(source_name.into(), source_contents.into())], None);
    let (compile_unit, errors) = compile::compile(&package_store, &[std_package_id], source_map);
    Compilation {
        package_store,
        std_package_id,
        ast_package: compile_unit.ast_package,
        names: compile_unit.names,
        tys: compile_unit.tys,
        package: compile_unit.package,
        source_map: compile_unit.sources,
        errors,
    }
}

pub(crate) fn whats_next(source: &str, cursor_offset: u32) -> Vec<CompletionConstraint> {
    compile::whats_next(source, cursor_offset)
}

pub(crate) fn span_contains(span: Span, offset: u32) -> bool {
    offset >= span.lo && offset < span.hi
}

pub(crate) fn map_offset(source_map: &SourceMap, source_name: &str, source_offset: u32) -> u32 {
    source_map
        .find_by_name(source_name)
        .expect("source should exist in the source map")
        .offset
        + source_offset
}
