// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#[cfg(test)]
mod tests;

use crate::qsc_utils::{self, map_offset, Compilation};
use qsc::completion::{gather_names, GatherOptions, Prediction};
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
    let offset = map_offset(&compilation.unit.sources, source_name, offset);

    let source = compilation
        .unit
        .sources
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
            Prediction::Path => {
                names_added.get_or_insert_with(|| {
                    let (names, namespaces) = gather_names(
                        &compilation.package_store,
                        &[compilation.std_package_id],
                        &compilation.unit.ast.package,
                        offset,
                        &GatherOptions::NamespacesAndTerms,
                    );
                    builder.push_completions(
                        names
                            .unwrap_or_default()
                            .iter()
                            .map(std::convert::AsRef::as_ref),
                        CompletionItemKind::Function,
                    );
                    namespaces_added.get_or_insert_with(|| {
                        builder.push_completions(
                            namespaces.iter().map(std::convert::AsRef::as_ref),
                            CompletionItemKind::Module,
                        );
                    });
                });
            }
            Prediction::Ty => {
                types_added.get_or_insert_with(|| {
                    let (names, namespaces) = gather_names(
                        &compilation.package_store,
                        &[compilation.std_package_id],
                        &compilation.unit.ast.package,
                        offset,
                        &GatherOptions::NamespacesAndTypes,
                    );
                    builder.push_completions(
                        names
                            .unwrap_or_default()
                            .iter()
                            .map(std::convert::AsRef::as_ref),
                        CompletionItemKind::Interface,
                    );
                    namespaces_added.get_or_insert_with(|| {
                        builder.push_completions(
                            namespaces.iter().map(std::convert::AsRef::as_ref),
                            CompletionItemKind::Module,
                        );
                    });
                });
            }
            Prediction::Namespace => {
                let (_, namespaces) = gather_names(
                    &compilation.package_store,
                    &[compilation.std_package_id],
                    &compilation.unit.ast.package,
                    offset,
                    &GatherOptions::NamespacesAndTypes,
                );
                namespaces_added.get_or_insert_with(|| {
                    builder.push_completions(
                        namespaces.iter().map(std::convert::AsRef::as_ref),
                        CompletionItemKind::Module,
                    );
                });
            }
            Prediction::Keyword(keyword) => {
                if keywords_added.insert(keyword.to_string()) {
                    builder.push_completions([keyword].into_iter(), CompletionItemKind::Keyword);
                }
            }
            Prediction::Qubit => {
                builder.push_completions(["Qubit"].into_iter(), CompletionItemKind::Interface);
            }
            Prediction::Attr => {
                // Only known attribute is EntryPoint
                builder.push_completions(["EntryPoint"].into_iter(), CompletionItemKind::Interface);
            }
            _ => {} // Prediction::Field => {
                    //     builder.push_completions(["bogus_field"].into_iter(), CompletionItemKind::Function);
                    // }
                    // Prediction::TyParam => {
                    //     builder
                    //         .push_completions(["'bogus_param"].into_iter(), CompletionItemKind::Interface);
                    // }
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
}
