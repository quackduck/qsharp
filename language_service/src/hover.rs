// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::display::{parse_doc_for_param, parse_doc_for_summary, CodeDisplay};
use crate::protocol::{self, Hover};
use crate::qsc_utils::{find_ident, find_item, map_offset, Compilation};
use crate::visitor::{node_by_offset, Node, NodeKind};
use qsc::{ast, hir, resolve, SourceMap};
use std::fmt::Display;
use std::rc::Rc;

pub(crate) fn get_hover(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> Option<Hover> {
    // Map the file offset into a SourceMap offset
    let offset = map_offset(&compilation.unit.sources, source_name, offset);
    let package = &compilation.unit.ast.package;

    node_by_offset(offset, package)
        .and_then(|node| get_hover_for_node(compilation, &CodeDisplay { compilation }, &node))
}

enum LocalKind {
    Param,
    LambdaParam,
    Local,
}

#[allow(clippy::too_many_lines)]
fn get_hover_for_node(
    compilation: &Compilation,
    display: &CodeDisplay,
    node: &Node,
) -> Option<Hover> {
    match &node.kind {
        NodeKind::CallableDeclName(decl, doc, current_namespace) => {
            let contents =
                display_callable(doc, current_namespace, display.ast_callable_decl(decl));
            Some(Hover {
                contents,
                span: protocol_span(node.span, &compilation.unit.sources),
            })
        }
        NodeKind::TyDefName(ident, def) => {
            let contents = markdown_fenced_block(display.ident_ty_def(ident, def));
            Some(Hover {
                contents,
                span: protocol_span(node.span, &compilation.unit.sources),
            })
        }
        NodeKind::TyDefFieldName(ident, ty) => {
            let contents = markdown_fenced_block(display.ident_ty(ident, ty));
            Some(Hover {
                contents,
                span: protocol_span(node.span, &compilation.unit.sources),
            })
        }
        NodeKind::PatBindName(context) => {
            let code_block =
                markdown_fenced_block(display.ident_ty_id(context.node, context.pat.id));
            let kind = if context.in_params {
                LocalKind::Param
            } else if context.in_lambda_params {
                LocalKind::LambdaParam
            } else {
                LocalKind::Local
            };
            let mut callable_name = Rc::from("");
            if let Some(decl) = context.current_callable {
                callable_name = decl.name.name.clone();
            }
            let contents = display_local(
                &kind,
                &code_block,
                &context.node.name,
                &callable_name,
                context.current_item_doc,
            );
            Some(Hover {
                contents,
                span: protocol_span(node.span, &compilation.unit.sources),
            })
        }
        NodeKind::ExprField(expr) => {
            let contents = markdown_fenced_block(display.ident_ty_id(expr.field, expr.node.id));
            Some(Hover {
                contents,
                span: protocol_span(expr.field.span, &compilation.unit.sources),
            })
        }
        NodeKind::Path(path) => {
            let res = compilation.unit.ast.names.get(path.node.id);
            if let Some(res) = res {
                match &res {
                    resolve::Res::Item(item_id) => {
                        if let (Some(item), Some(package)) = find_item(compilation, item_id) {
                            let ns = item
                                .parent
                                .and_then(|parent_id| package.items.get(parent_id))
                                .map_or_else(
                                    || Rc::from(""),
                                    |parent| match &parent.kind {
                                        qsc::hir::ItemKind::Namespace(namespace, _) => {
                                            namespace.name.clone()
                                        }
                                        _ => Rc::from(""),
                                    },
                                );

                            let contents = match &item.kind {
                                hir::ItemKind::Callable(decl) => display_callable(
                                    &item.doc,
                                    &ns,
                                    display.hir_callable_decl(decl),
                                ),
                                hir::ItemKind::Namespace(_, _) => {
                                    panic!(
                                        "Reference node should not refer to a namespace: {}",
                                        path.node.id
                                    )
                                }
                                hir::ItemKind::Ty(_, udt) => {
                                    markdown_fenced_block(display.hir_udt(udt))
                                }
                            };
                            return Some(Hover {
                                contents,
                                span: protocol_span(node.span, &compilation.unit.sources),
                            });
                        }
                    }
                    resolve::Res::Local(node_id) => {
                        let mut local_name = Rc::from("");
                        let mut callable_name = Rc::from("");
                        if let Some(curr) = path.current_callable {
                            callable_name = curr.name.name.clone();
                            if let Some(ident) = find_ident(node_id, curr) {
                                local_name = ident.name.clone();
                            }
                        }

                        let code_block =
                            markdown_fenced_block(display.path_ty_id(path.node, *node_id));
                        let kind = if is_param(
                            &curr_callable_to_params(path.current_callable),
                            *node_id,
                        ) {
                            LocalKind::Param
                        } else if is_param(&path.lambda_params, *node_id) {
                            LocalKind::LambdaParam
                        } else {
                            LocalKind::Local
                        };
                        let contents = display_local(
                            &kind,
                            &code_block,
                            &local_name,
                            &callable_name,
                            path.current_item_doc,
                        );
                        return Some(Hover {
                            contents,
                            span: protocol_span(node.span, &compilation.unit.sources),
                        });
                    }
                    _ => {}
                };
            };
            None
        }
    }
}

fn protocol_span(span: qsc::Span, source_map: &SourceMap) -> protocol::Span {
    // Note that lo and hi offsets will usually be the same as
    // the span will usually come from a single source.
    let lo_offset = source_map
        .find_by_offset(span.lo)
        .expect("source should exist for offset")
        .offset;
    let hi_offset = source_map
        .find_by_offset(span.hi)
        .expect("source should exist for offset")
        .offset;
    protocol::Span {
        start: span.lo - lo_offset,
        end: span.hi - hi_offset,
    }
}

fn curr_callable_to_params(curr_callable: Option<&ast::CallableDecl>) -> Vec<&ast::Pat> {
    match curr_callable {
        Some(decl) => match &*decl.body {
            ast::CallableBody::Block(_) => vec![decl.input.as_ref()],
            ast::CallableBody::Specs(spec_decls) => {
                let mut pats = spec_decls
                    .iter()
                    .filter_map(|spec| match &spec.body {
                        ast::SpecBody::Gen(_) => None,
                        ast::SpecBody::Impl(input, _) => Some(input.as_ref()),
                    })
                    .collect::<Vec<&ast::Pat>>();
                pats.push(decl.input.as_ref());
                pats
            }
        },
        None => vec![],
    }
}

fn is_param(param_pats: &[&ast::Pat], node_id: ast::NodeId) -> bool {
    fn find_in_pat(pat: &ast::Pat, node_id: ast::NodeId) -> bool {
        match &*pat.kind {
            ast::PatKind::Bind(ident, _) => node_id == ident.id,
            ast::PatKind::Discard(_) | ast::PatKind::Elided => false,
            ast::PatKind::Paren(inner) => find_in_pat(inner, node_id),
            ast::PatKind::Tuple(inner) => inner.iter().any(|x| find_in_pat(x, node_id)),
        }
    }

    param_pats.iter().any(|pat| find_in_pat(pat, node_id))
}

fn display_local(
    param_kind: &LocalKind,
    markdown: &String,
    local_name: &str,
    callable_name: &str,
    callable_doc: &str,
) -> String {
    match param_kind {
        LocalKind::Param => {
            let param_doc = parse_doc_for_param(callable_doc, local_name);
            with_doc(
                &param_doc,
                format!("parameter of `{callable_name}`\n{markdown}",),
            )
        }
        LocalKind::LambdaParam => format!("lambda parameter\n{markdown}"),
        LocalKind::Local => format!("local\n{markdown}"),
    }
}

fn display_callable(doc: &str, namespace: &str, code: impl Display) -> String {
    let summary = parse_doc_for_summary(doc);

    let mut code = if namespace.is_empty() {
        code.to_string()
    } else {
        format!("{namespace}\n{code}")
    };
    code = markdown_fenced_block(code);
    with_doc(&summary, code)
}

fn with_doc(doc: &String, code: impl Display) -> String {
    if doc.is_empty() {
        code.to_string()
    } else {
        format!("{code}---\n{doc}\n")
    }
}

fn markdown_fenced_block(code: impl Display) -> String {
    format!(
        "```qsharp
{code}
```
"
    )
}
