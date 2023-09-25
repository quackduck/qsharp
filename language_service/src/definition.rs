// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::protocol::Definition;
use crate::qsc_utils::{find_ident, find_item, map_offset, Compilation, QSHARP_LIBRARY_URI_SCHEME};
use crate::visitor::{node_by_offset, Node, NodeKind};
use qsc::hir::PackageId;
use qsc::{hir, resolve};

pub(crate) fn get_definition(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Option<Definition> {
    // Map the file offset into a SourceMap offset
    let offset = map_offset(&compilation.unit.sources, source_name, offset);
    let ast_package = &compilation.unit.ast;

    node_by_offset(offset, &ast_package.package)
        .and_then(|node| get_definition_for_node(compilation, &node))
        .map(|(name, offset)| Definition {
            source: name,
            offset,
        })
}

fn definition_from_position(
    compilation: &Compilation,
    lo: u32,
    package_id: Option<PackageId>,
) -> (std::string::String, u32) {
    let source_map = match package_id {
        Some(id) => {
            &compilation
                .package_store
                .get(id)
                .unwrap_or_else(|| panic!("package should exist for id {id}"))
                .sources
        }
        None => &compilation.unit.sources,
    };
    let source = source_map
        .find_by_offset(lo)
        .expect("source should exist for offset");
    // Note: Having a package_id means the position references a foreign package.
    // Currently the only supported foreign packages are our library packages,
    // URI's to which need to include our custom library scheme.
    let source_name = match package_id {
        Some(_) => format!("{}:{}", QSHARP_LIBRARY_URI_SCHEME, source.name),
        None => source.name.to_string(),
    };

    (source_name, lo - source.offset)
}

fn get_definition_for_node(compilation: &Compilation, node: &Node) -> Option<(String, u32)> {
    match &node.kind {
        NodeKind::CallableDeclName(_, _, _)
        | NodeKind::TyDefName(_, _)
        | NodeKind::TyDefFieldName(_, _)
        | NodeKind::PatBindName(_) => {
            Some(definition_from_position(compilation, node.span.lo, None))
        }
        NodeKind::ExprField(expr) => {
            if let Some(hir::ty::Ty::Udt(res)) = compilation.unit.ast.tys.terms.get(expr.record.id)
            {
                match res {
                    hir::Res::Item(item_id) => {
                        if let (Some(item), _) = find_item(compilation, item_id) {
                            match &item.kind {
                                hir::ItemKind::Ty(_, udt) => {
                                    if let Some(field) = udt.find_field_by_name(&expr.field.name) {
                                        let span = field
                                            .name_span
                                            .expect("field found via name should have a name");
                                        return Some(definition_from_position(
                                            compilation,
                                            span.lo,
                                            item_id.package,
                                        ));
                                    }
                                }
                                _ => panic!("UDT has invalid resolution."),
                            }
                        }
                    }
                    _ => panic!("UDT has invalid resolution."),
                }
            };
            None
        }
        NodeKind::Path(path) => {
            let res = compilation.unit.ast.names.get(path.node.id);
            if let Some(res) = res {
                match &res {
                    resolve::Res::Item(item_id) => {
                        if let (Some(item), _) = find_item(compilation, item_id) {
                            let lo = match &item.kind {
                                hir::ItemKind::Callable(decl) => decl.name.span.lo,
                                hir::ItemKind::Namespace(_, _) => {
                                    panic!(
                                        "Reference node should not refer to a namespace: {}",
                                        path.node.id
                                    )
                                }
                                hir::ItemKind::Ty(ident, _) => ident.span.lo,
                            };
                            return Some(definition_from_position(
                                compilation,
                                lo,
                                item_id.package,
                            ));
                        }
                    }
                    resolve::Res::Local(node_id) => {
                        if let Some(curr) = path.current_callable {
                            {
                                if let Some(ident) = find_ident(node_id, curr) {
                                    return Some(definition_from_position(
                                        compilation,
                                        ident.span.lo,
                                        None,
                                    ));
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
            None
        }
    }
}
