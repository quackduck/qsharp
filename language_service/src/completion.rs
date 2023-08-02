// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::qsc_utils::{self, map_offset, span_contains, Compilation};
use qsc::{
    gather_names,
    hir::{
        visit::{walk_item, Visitor},
        ItemKind, {Block, Item},
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

#[allow(clippy::too_many_lines)]
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

    let mut items = Vec::new();

    // Just making sure certain things only get added once
    // whats_next is still a bit fuzzy and returns the same
    // constraint multiple times sometimes
    let mut names_added = None;
    let mut types_added = None;
    let mut namespaces_added = None;
    let mut keywords_added = HashSet::new();

    // there's some contradiction here because we're using one source file
    // but we'll need the whole source map at some point. Makes no difference
    // rn b/c there's only ever one file
    for completion_constraint in qsc_utils::whats_next(&source.contents, offset) {
        match completion_constraint {
            qsc::Prediction::Path => {
                names_added.get_or_insert_with(|| {
                    let (names, namespaces) = gather_names(
                        &compilation.package_store,
                        &[compilation.std_package_id],
                        &compilation.ast_package,
                        offset,
                        &GatherOptions::NamespacesAndTerms,
                    );
                    for name in names.unwrap_or_default() {
                        items.push(CompletionItem {
                            label: name.to_string(),
                            kind: CompletionItemKind::Function,
                        });
                    }
                    namespaces_added.get_or_insert_with(|| {
                        for namespace in namespaces {
                            items.push(CompletionItem {
                                label: namespace.to_string(),
                                kind: CompletionItemKind::Module,
                            });
                        }
                    });
                });
            }
            qsc::Prediction::Ty => {
                types_added.get_or_insert_with(|| {
                    let (names, namespaces) = gather_names(
                        &compilation.package_store,
                        &[compilation.std_package_id],
                        &compilation.ast_package,
                        offset,
                        &GatherOptions::NamespacesAndTypes,
                    );
                    for name in names.unwrap_or_default() {
                        items.push(CompletionItem {
                            label: name.to_string(),
                            kind: CompletionItemKind::Interface,
                        });
                    }
                    namespaces_added.get_or_insert_with(|| {
                        for namespace in namespaces {
                            items.push(CompletionItem {
                                label: namespace.to_string(),
                                kind: CompletionItemKind::Module,
                            });
                        }
                    });
                });
            }
            qsc::Prediction::Namespace => {
                let (_, namespaces) = gather_names(
                    &compilation.package_store,
                    &[compilation.std_package_id],
                    &compilation.ast_package,
                    offset,
                    &GatherOptions::NamespacesAndTypes,
                );
                namespaces_added.get_or_insert_with(|| {
                    for namespace in namespaces {
                        items.push(CompletionItem {
                            label: namespace.to_string(),
                            kind: CompletionItemKind::Module,
                        });
                    }
                });
            }
            qsc::Prediction::Qubit => {
                items.push(CompletionItem {
                    label: "Qubit".to_string(),
                    kind: CompletionItemKind::Interface,
                });
            }
            qsc::Prediction::Keyword(keyword) => {
                if keywords_added.insert(keyword.to_string()) {
                    items.push(CompletionItem {
                        label: keyword.to_string(),
                        kind: CompletionItemKind::Keyword,
                    });
                }
            }
            qsc::Prediction::Field => {
                items.push(CompletionItem {
                    label: "[field options]".to_string(),
                    kind: CompletionItemKind::Issue,
                });
            }
            qsc::Prediction::Attr => {
                items.push(CompletionItem {
                    label: "[attr options]".to_string(),
                    kind: CompletionItemKind::Issue,
                });
            }
            qsc::Prediction::TyParam => {
                items.push(CompletionItem {
                    label: "[typaram options]".to_string(),
                    kind: CompletionItemKind::Issue,
                });
            }
            qsc::Prediction::Debug(s) => {
                items.push(CompletionItem {
                    label: format!("~~ {s}"),
                    kind: CompletionItemKind::Issue,
                });
            }
            qsc::Prediction::Other(t) => {
                items.push(CompletionItem {
                    label: format!("~ {t}"),
                    kind: CompletionItemKind::Issue,
                });
            }
        }
    }

    CompletionList { items }
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
