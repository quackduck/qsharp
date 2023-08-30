// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use crate::{protocol::Position, PositionEncodingKind};
use qsc::{
    compile::{self, Error},
    hir::{Item, ItemId, Package, PackageId},
    utf16, CompileUnit, PackageStore, PackageType, SourceMap, Span, TargetProfile,
};

pub(crate) const QSHARP_LIBRARY_URI_SCHEME: &str = "qsharp-library-source";

/// Represents an immutable compilation state that can be used
/// to implement language service features.
pub(crate) struct Compilation {
    pub package_store: PackageStore,
    pub std_package_id: PackageId,
    pub unit: CompileUnit,
    pub errors: Vec<Error>,
}

pub(crate) fn compile_document(
    source_name: &str,
    source_contents: &str,
    package_type: PackageType,
) -> Compilation {
    let mut package_store = PackageStore::new(compile::core());
    let std_package_id = package_store.insert(compile::std(&package_store, TargetProfile::Full));

    // Source map only contains the current document.
    let source_map = SourceMap::new([(source_name.into(), source_contents.into())], None);
    let (unit, errors) = compile::compile(
        &package_store,
        &[std_package_id],
        source_map,
        package_type,
        TargetProfile::Full,
    );
    Compilation {
        package_store,
        std_package_id,
        unit,
        errors,
    }
}

pub(crate) fn span_contains(span: Span, offset: u32) -> bool {
    offset >= span.lo && offset < span.hi
}

pub(crate) fn position(
    position_encoding_kind: PositionEncodingKind,
    source_map: &SourceMap,
    // should return source name too, useful for example for diagnostics
    offset: u32,
) -> Position {
    match position_encoding_kind {
        PositionEncodingKind::Utf8Offset => Position::Utf8Offset(offset),
        PositionEncodingKind::Utf16LineColumn => {
            let source = source_map
                .find_by_offset(offset)
                .expect("expected offset to be in a source");
            Position::Utf16LineColumn(utf16::Position::utf16(
                source.contents.as_ref(),
                offset - source.offset,
            ))
        }
    }
}

pub(crate) fn map_position(
    source_map: &SourceMap,
    source_name: &str,
    source_position: &Position,
) -> u32 {
    let source = source_map
        .find_by_name(source_name)
        .expect("source should exist in the source map");
    let source_offset = match source_position {
        Position::Utf16LineColumn(p) => p.to_offset(source.contents.as_ref()),
        Position::Utf8Offset(o) => *o,
    };

    source.offset + source_offset
}

pub(crate) fn find_item<'a>(
    compilation: &'a Compilation,
    id: &ItemId,
) -> (Option<&'a Item>, Option<&'a Package>) {
    let package = if let Some(package_id) = id.package {
        match compilation.package_store.get(package_id) {
            Some(compilation) => &compilation.package,
            None => return (None, None),
        }
    } else {
        &compilation.unit.package
    };
    (package.items.get(id.item), Some(package))
}
