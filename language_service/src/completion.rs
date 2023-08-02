// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::display::CodeDisplay;
use crate::qsc_utils::{self, map_offset, span_contains, Compilation};
use crate::qsc_utils::{map_offset, span_contains, Compilation};
use qsc::ast::visit::{self, Visitor};
use qsc::hir::{ItemKind, Package, PackageId};
use qsc::{
    gather_names,
    hir::{
        visit::{walk_item, Visitor},
        ItemKind, {Block, Item},
    },
    GatherOptions,
};
use std::collections::HashSet;

#[derive(Copy, Clone, Debug, PartialEq)]
#[allow(clippy::module_name_repetitions)]
pub enum CompletionItemKind {
    // It would have been nice to match these enum values to the ones used by
    // VS Code and Monaco, but unfortunately those two disagree on the values.
    // So we define our own unique enum here to reduce confusion.
    Function,
    Interface,
    Keyword,
    Module,
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
    pub sort_text: Option<String>,
    pub detail: Option<String>,
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

    let mut builder = CompletionListBuilder::new();

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
                    builder.push_completions(
                        names.unwrap_or_default().map(|n| n.to_string()),
                        CompletionItemKind::Function,
                    );
                    namespaces_added.get_or_insert_with(|| {
                        builder.push_completions(
                            namespaces.map(|n| n.to_string()),
                            CompletionItemKind::Module,
                        );
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
                    builder.push_completions(
                        names.unwrap_or_default().map(|n| n.to_string()),
                        CompletionItemKind::Interface,
                    );
                    namespaces_added.get_or_insert_with(|| {
                        namespaces_added.get_or_insert_with(|| {
                            builder.push_completions(
                                namespaces.map(|n| n.to_string()),
                                CompletionItemKind::Module,
                            );
                        });
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
                    namespaces_added.get_or_insert_with(|| {
                        builder.push_completions(
                            namespaces.map(|n| n.to_string()),
                            CompletionItemKind::Module,
                        );
                    });
                });
            }
            qsc::Prediction::Keyword(keyword) => {
                if keywords_added.insert(keyword.to_string()) {
                    builder
                        .push_completions(vec![keyword.to_string()], CompletionItemKind::Keyword);
                }
            }
            qsc::Prediction::Qubit => {
                builder.push_completions(vec!["Qubit".to_string()], CompletionItemKind::Interface);
            }
            qsc::Prediction::Attr => {
                // Only known attribute is EntryPoint
                builder.push_completions(
                    vec!["EntryPoint".to_string()],
                    CompletionItemKind::Interface,
                );
            }
            qsc::Prediction::Field => {
                builder.push_completions(
                    vec!["bogus_field".to_string()],
                    CompletionItemKind::Function,
                );
            }
            qsc::Prediction::TyParam => {
                builder.push_completions(
                    vec!["'bogus_param".to_string()],
                    CompletionItemKind::Interface,
                );
            }
        }
    }

    CompletionList {
        items: builder.into_items(),
    }
}

struct CompletionListBuilder {
    current_sort_group: u32,
    items: Vec<CompletionItem>,
}

impl CompletionListBuilder {
    fn new() -> Self {
        CompletionListBuilder {
            current_sort_group: 1,
            items: Vec::new(),
        }
    }

    fn into_items(self) -> Vec<CompletionItem> {
        self.items
    }

    fn push_item_decl_keywords(&mut self) {
        static ITEM_KEYWORDS: [&str; 5] = ["operation", "open", "internal", "function", "newtype"];

        self.push_completions(ITEM_KEYWORDS.into_iter(), CompletionItemKind::Keyword);
    }

    fn push_namespace_keyword(&mut self) {
        self.push_completions(["namespace"].into_iter(), CompletionItemKind::Keyword);
    }

    fn push_types(&mut self) {
        static PRIMITIVE_TYPES: [&str; 10] = [
            "Qubit", "Int", "Unit", "Result", "Bool", "BigInt", "Double", "Pauli", "Range",
            "String",
        ];
        static FUNCTOR_KEYWORDS: [&str; 3] = ["Adj", "Ctl", "is"];

        self.push_completions(PRIMITIVE_TYPES.into_iter(), CompletionItemKind::Interface);
        self.push_completions(FUNCTOR_KEYWORDS.into_iter(), CompletionItemKind::Keyword);
    }

    fn push_globals(&mut self, compilation: &Compilation) {
        let current = &compilation.unit.package;
        let std = &compilation
            .package_store
            .get(compilation.std_package_id)
            .expect("expected to find std package")
            .package;
        let core = &compilation
            .package_store
            .get(PackageId::CORE)
            .expect("expected to find core package")
            .package;

        let display = CodeDisplay { compilation };

        self.push_sorted_completions(
            Self::get_callables(current, &display),
            CompletionItemKind::Function,
        );
        self.push_sorted_completions(
            Self::get_callables(std, &display),
            CompletionItemKind::Function,
        );
        self.push_sorted_completions(
            Self::get_callables(core, &display),
            CompletionItemKind::Function,
        );
        self.push_completions(Self::get_namespaces(current), CompletionItemKind::Module);
        self.push_completions(Self::get_namespaces(std), CompletionItemKind::Module);
        self.push_completions(Self::get_namespaces(core), CompletionItemKind::Module);
    }

    fn push_stmt_keywords(&mut self) {
        static STMT_KEYWORDS: [&str; 5] = ["let", "return", "use", "mutable", "borrow"];

        self.push_completions(STMT_KEYWORDS.into_iter(), CompletionItemKind::Keyword);
    }

    fn push_expr_keywords(&mut self) {
        static EXPR_KEYWORDS: [&str; 11] = [
            "if", "for", "in", "within", "apply", "repeat", "until", "fixup", "set", "while",
            "fail",
        ];

        self.push_completions(EXPR_KEYWORDS.into_iter(), CompletionItemKind::Keyword);
    }

    /// Each invocation of this function increments the sort group so that
    /// in the eventual completion list, the groups of items show up in the
    /// order they were added.
    /// The items are then sorted according to the input list order (not alphabetical)
    pub fn push_completions<'a>(
        &mut self,
        iter: impl Iterator<Item = &'a str>,
        kind: CompletionItemKind,
    ) {
        let mut current_sort_prefix = 0;

        self.items.extend(iter.map(|name| CompletionItem {
            label: name.to_string(),
            kind,
            sort_text: {
                current_sort_prefix += 1;
                Some(format!(
                    "{:02}{:02}{}",
                    self.current_sort_group, current_sort_prefix, name
                ))
            },
            detail: None,
        }));

        self.current_sort_group += 1;
    }

    /// Push a group of completions that are themselves sorted into subgroups
    fn push_sorted_completions<'a>(
        &mut self,
        iter: impl Iterator<Item = (&'a str, Option<String>, u32)>,
        kind: CompletionItemKind,
    ) {
        self.items
            .extend(iter.map(|(name, detail, item_sort_group)| CompletionItem {
                label: name.to_string(),
                kind,
                sort_text: Some(format!(
                    "{:02}{:02}{}",
                    self.current_sort_group, item_sort_group, name
                )),
                detail,
            }));

        self.current_sort_group += 1;
    }

    fn get_callables<'a>(
        package: &'a Package,
        display: &'a CodeDisplay,
    ) -> impl Iterator<Item = (&'a str, Option<String>, u32)> {
        package.items.values().filter_map(|i| match &i.kind {
            ItemKind::Callable(callable_decl) => Some({
                let name = callable_decl.name.name.as_ref();
                let detail = Some(display.hir_callable_decl(callable_decl).to_string());
                // Everything that starts with a __ goes last in the list
                let sort_group = u32::from(name.starts_with("__"));
                (name, detail, sort_group)
            }),
            _ => None,
        })
    }

    fn get_namespaces(package: &Package) -> impl Iterator<Item = &str> {
        package.items.values().filter_map(|i| match &i.kind {
            ItemKind::Namespace(namespace, _) => Some(namespace.name.as_ref()),
            _ => None,
        })
    }
}
