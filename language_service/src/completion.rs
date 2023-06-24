// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::qsc_utils::{self, map_offset, span_contains, Compilation};
use qsc::{
    gather_names,
    hir::{
        visit::{walk_item, Visitor},
        ItemKind, {Block, Item, Package},
    },
    GatherOptions,
};
use std::collections::HashSet;

// It would have been nice to match these enum values to the ones used by
// VS Code and Monaco, but unfortunately those two disagree on the values.
// So we define our own unique enum here to reduce confusion.
#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub enum CompletionItemKind {
    Function,
    Module,
    Keyword,
    Issue,
    Interface,
}

#[derive(Debug)]
#[allow(clippy::module_name_repetitions)]
pub struct CompletionList {
    pub items: Vec<CompletionItem>,
}

#[derive(Clone, Debug, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub struct CompletionItem {
    pub label: String,
    pub kind: CompletionItemKind,
}

pub(crate) fn get_completions(
    compilation: &Compilation,
    source_name: &str,
    offset: u32,
) -> CompletionList {
    // Map the file offset into a SourceMap offset
    let offset = map_offset(&compilation.source_map, source_name, offset);

    let source = compilation
        .source_map
        .find_by_name(source_name)
        .expect("source not found");
    let truncated_source = &source.contents[..offset as usize];

    let mut items = Vec::new();

    for completion_constraint in qsc_utils::whats_next(truncated_source) {
        add_completions(completion_constraint, compilation, offset, &mut items);
    }

    CompletionList { items }
}

#[allow(clippy::too_many_lines)]
fn add_completions(
    constraint: qsc::CompletionConstraint,
    compilation: &Compilation,
    offset: u32,
    items: &mut Vec<CompletionItem>,
) {
    match constraint {
        qsc::CompletionConstraint::Path => {
            let (names, namespaces) = gather_names(
                &compilation.package_store,
                &[compilation.std_package_id],
                &compilation.ast_package,
                offset,
                &GatherOptions::NamespacesAndTerms,
            );
            for name in names {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: CompletionItemKind::Function,
                });
            }
            for namespace in namespaces {
                items.push(CompletionItem {
                    label: namespace.to_string(),
                    kind: CompletionItemKind::Module,
                });
            }
        }
        qsc::CompletionConstraint::Ty => {
            let (names, namespaces) = gather_names(
                &compilation.package_store,
                &[compilation.std_package_id],
                &compilation.ast_package,
                offset,
                &GatherOptions::NamespacesAndTypes,
            );
            for name in names {
                items.push(CompletionItem {
                    label: name.to_string(),
                    kind: CompletionItemKind::Interface,
                });
            }
            for namespace in namespaces {
                items.push(CompletionItem {
                    label: namespace.to_string(),
                    kind: CompletionItemKind::Module,
                });
            }
        }
        qsc::CompletionConstraint::Namespace => {
            let (_, namespaces) = gather_names(
                &compilation.package_store,
                &[compilation.std_package_id],
                &compilation.ast_package,
                offset,
                &GatherOptions::NamespacesAndTypes,
            );
            for namespace in namespaces {
                items.push(CompletionItem {
                    label: namespace.to_string(),
                    kind: CompletionItemKind::Module,
                });
            }
        }
        qsc::CompletionConstraint::Qubit => {
            items.push(CompletionItem {
                label: "Qubit".to_string(),
                kind: CompletionItemKind::Interface,
            });
        }
        qsc::CompletionConstraint::Keyword(keyword) => {
            items.push(CompletionItem {
                label: keyword,
                kind: CompletionItemKind::Keyword,
            });
        }
        qsc::CompletionConstraint::Field => {
            items.push(CompletionItem {
                label: "[field options]".to_string(),
                kind: CompletionItemKind::Issue,
            });
        }
        qsc::CompletionConstraint::Attr => {
            items.push(CompletionItem {
                label: "[attr options]".to_string(),
                kind: CompletionItemKind::Issue,
            });
        }
        qsc::CompletionConstraint::TyParam => {
            items.push(CompletionItem {
                label: "[typaram options]".to_string(),
                kind: CompletionItemKind::Issue,
            });
        }
        qsc::CompletionConstraint::Binding => {
            items.push(CompletionItem {
                label: "~BINDING".to_string(),
                kind: CompletionItemKind::Issue,
            });
        }
        qsc::CompletionConstraint::Debug(s) => {
            items.push(CompletionItem {
                label: format!("~~DEBUG {s}"),
                kind: CompletionItemKind::Issue,
            });
        }
        qsc::CompletionConstraint::Other(t) => {
            items.push(CompletionItem {
                label: format!("~TOKEN {t}"),
                kind: CompletionItemKind::Issue,
            });
        }
    }
}

struct NamespaceCollector {
    namespaces: HashSet<String>,
}

impl Visitor<'_> for NamespaceCollector {
    fn visit_item(&mut self, item: &Item) {
        if let ItemKind::Namespace(ident, _) = &item.kind {
            // Collect namespaces
            self.namespaces.insert(ident.name.to_string());
        }
        walk_item(self, item);
    }
}

struct ContextFinder {
    offset: u32,
    context: Context,
}

#[derive(Debug, PartialEq)]
enum Context {
    NoCompilation,
    TopLevel,
    Namespace,
    Block,
    NotSignificant,
}

impl Visitor<'_> for ContextFinder {
    fn visit_item(&mut self, item: &Item) {
        if span_contains(item.span, self.offset) {
            self.context = match &item.kind {
                ItemKind::Namespace(..) => Context::Namespace,
                _ => Context::NotSignificant,
            }
        }

        walk_item(self, item);
    }

    fn visit_block(&mut self, block: &Block) {
        if span_contains(block.span, self.offset) {
            self.context = Context::Block;
        }
    }
}

fn callable_names_from_package(package: &Package) -> Vec<CompletionItem> {
    package
        .items
        .values()
        .filter_map(|i| match &i.kind {
            ItemKind::Callable(callable_decl) => Some(CompletionItem {
                label: callable_decl.name.name.to_string(),
                kind: CompletionItemKind::Function,
            }),
            _ => None,
        })
        .collect::<Vec<_>>()
}
