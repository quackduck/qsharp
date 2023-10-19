// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![warn(clippy::mod_module_files, clippy::pedantic, clippy::unwrap_used)]
#![allow(clippy::missing_errors_doc, clippy::missing_panics_doc)]

mod compilation;
pub mod completion;
pub mod definition;
mod display;
pub mod hover;
pub mod protocol;
mod qsc_utils;
pub mod rename;
pub mod signature_help;
#[cfg(test)]
mod test_utils;
#[cfg(test)]
mod tests;

use compilation::Compilation;
use log::trace;
use miette::Diagnostic;
use protocol::{
    CompletionList, Definition, DiagnosticUpdate, Hover, SignatureHelp,
    WorkspaceConfigurationUpdate,
};
use qsc::{compile::Error, PackageType, TargetProfile};
use std::{
    collections::{HashMap, HashSet},
    mem::take,
    sync::Arc,
};

type CompilationUri = Arc<str>;
type DocumentUri = Arc<str>;

pub struct LanguageService<'a> {
    /// Workspace configuration can include compiler settings
    /// that affect error checking and other language server behavior.
    /// Currently these settings apply to all documents in the
    /// workspace. Per-document configurations are not supported.
    configuration: WorkspaceConfiguration,
    /// Currently each Q# file is its own unique compilation.
    /// For notebooks, each notebook is a compilation, each cell is a document
    /// within that compilation.
    /// CompilationUri is the document uri for single-file compilations,
    /// notebook uri for notebooks
    /// It's also where project-level errors get reported
    compilations: HashMap<CompilationUri, Compilation>,
    /// All documents known to the client.
    /// (cell uri -> notebook uri, or identity in the case of single-file compilation)
    /// Not all documents that make up the compilation need to be in this map -
    /// only the ones known to the client.
    open_documents: HashMap<DocumentUri, OpenDocument>,
    /// Documents that errors were published to. We need to keep track
    /// of this so we can clear errors from them when documents are removed
    /// from a compilation or when a recompilation occurs.
    documents_with_errors: HashSet<DocumentUri>,
    /// Callback which will receive diagnostics (compilation errors)
    /// whenever a (re-)compilation occurs.
    diagnostics_receiver: Box<dyn Fn(DiagnosticUpdate) + 'a>,
}

#[derive(Debug)]
struct WorkspaceConfiguration {
    pub target_profile: TargetProfile,
    pub package_type: PackageType,
}

impl Default for WorkspaceConfiguration {
    fn default() -> Self {
        Self {
            target_profile: TargetProfile::Full,
            package_type: PackageType::Exe,
        }
    }
}

#[derive(Debug)]
struct OpenDocument {
    // TODO: versions are supposed to be associated with documents not compilations (??)
    /// This version is the document version provided by the client.
    /// It increases strictly with each text change, though this knowledge should
    /// not be important. The version is only ever used when publishing
    /// diagnostics to help the client associate the list of diagnostics
    /// with a snapshot of the document.
    pub version: u32,
    pub compilation: CompilationUri,
}

impl<'a> LanguageService<'a> {
    pub fn new(diagnostics_receiver: impl Fn(DiagnosticUpdate) + 'a) -> Self {
        LanguageService {
            configuration: WorkspaceConfiguration::default(),
            compilations: HashMap::new(),
            open_documents: HashMap::new(),
            documents_with_errors: HashSet::new(),
            diagnostics_receiver: Box::new(diagnostics_receiver),
        }
    }

    /// Updates the workspace configuration. If any compiler settings are updated,
    /// a recompilation may be triggered, which will result in a new set of diagnostics
    /// being published.
    pub fn update_configuration(&mut self, configuration: &WorkspaceConfigurationUpdate) {
        trace!("update_configuration: {configuration:?}");

        let need_recompile = self.apply_configuration(configuration);

        // Some configuration options require a recompilation as they impact error checking
        if need_recompile {
            self.recompile_all();
        }
    }

    /// Indicates that the document has been opened or the source has been updated.
    /// This should be called before any language service requests have been made
    /// for the document, typically when the document is first opened in the editor.
    /// It should also be called whenever the source code is updated.
    ///
    /// LSP: textDocument/didOpen, textDocument/didChange
    pub fn update_document(&mut self, uri: &str, version: u32, text: &str) {
        trace!("update_document: {uri} {version}");
        let compilation = Compilation::new_open_document(
            uri,
            text,
            self.configuration.package_type,
            self.configuration.target_profile,
        );

        // Associate each known document with a separate compilation.
        let uri: Arc<str> = uri.into();
        self.compilations.insert(uri.clone(), compilation);
        self.open_documents.insert(
            uri.clone(),
            OpenDocument {
                version,
                compilation: uri,
            },
        );
        self.publish_diagnostics();
    }

    /// Indicates that the client is no longer interested in the document,
    /// typically occurs when the document is closed in the editor.
    ///
    /// LSP: textDocument/didClose
    pub fn close_document(&mut self, uri: &str) {
        trace!("close_document: {uri}");

        self.compilations.remove(uri);
        self.open_documents.remove(uri);

        self.publish_diagnostics();
    }

    /// The uri refers to the notebook itself, not any of the individual cells.
    ///
    /// This function expects all Q# content in the notebook every time
    /// it is called, not just the changed cells.
    ///
    /// At this layer we expect the client to have stripped
    /// off all non-Q# content, including Python cells and lines
    /// containing the "%%qsharp" cell magic.
    ///
    /// LSP: notebookDocument/didOpen, notebookDocument/didChange
    pub fn update_notebook_document(
        &mut self,
        notebook_uri: &str,
        cells: &[(&str, u32, &str)], // uri, version, text - basically  DidChangeTextDocumentParams
    ) {
        trace!("update_notebook_document: {notebook_uri}");
        let compilation = Compilation::new_notebook(cells.iter().map(|c| (c.0, c.2)));

        let compilation_id: Arc<str> = notebook_uri.into();
        self.compilations
            .insert(compilation_id.clone(), compilation);

        for (cell_uri, version, _) in cells {
            self.open_documents.insert(
                (*cell_uri).into(),
                OpenDocument {
                    version: *version,
                    compilation: compilation_id.clone(),
                },
            );
        }

        self.publish_diagnostics();
    }

    /// Indicates that the client is no longer interested in the notebook.
    ///
    /// LSP: notebookDocument/didClose
    pub fn close_notebook_document<'b>(
        &mut self,
        uri: &str,
        cell_uris: impl Iterator<Item = &'b str>,
    ) {
        trace!("close_document: {uri}");

        for cell_uri in cell_uris {
            self.open_documents.remove(cell_uri);
        }

        self.compilations.remove(uri);

        self.publish_diagnostics();
    }

    /// LSP: textDocument/completion
    #[must_use]
    pub fn get_completions(&self, uri: &str, offset: u32) -> CompletionList {
        self.document_op(completion::get_completions, "get_completions", uri, offset)
    }

    /// LSP: textDocument/definition
    #[must_use]
    pub fn get_definition(&self, uri: &str, offset: u32) -> Option<Definition> {
        self.document_op(definition::get_definition, "get_definition", uri, offset)
    }

    /// LSP: textDocument/hover
    #[must_use]
    pub fn get_hover(&self, uri: &str, offset: u32) -> Option<Hover> {
        self.document_op(hover::get_hover, "get_hover", uri, offset)
    }

    /// LSP textDocument/signatureHelp
    #[must_use]
    pub fn get_signature_help(&self, uri: &str, offset: u32) -> Option<SignatureHelp> {
        self.document_op(
            signature_help::get_signature_help,
            "get_signature_help",
            uri,
            offset,
        )
    }

    /// LSP: textDocument/rename
    #[must_use]
    pub fn get_rename(&self, uri: &str, offset: u32) -> Vec<protocol::Span> {
        self.document_op(rename::get_rename, "get_rename", uri, offset)
    }

    /// LSP: textDocument/prepareRename
    #[must_use]
    pub fn prepare_rename(&self, uri: &str, offset: u32) -> Option<(protocol::Span, String)> {
        self.document_op(rename::prepare_rename, "prepare_rename", uri, offset)
    }

    /// Executes an operation that takes a document uri and offset, using the current compilation for that document
    fn document_op<F, T>(&self, op: F, op_name: &str, uri: &str, offset: u32) -> T
    where
        F: Fn(&Compilation, &str, u32) -> T,
        T: std::fmt::Debug,
    {
        trace!("{op_name}: uri: {uri}, offset: {offset}");
        let compilation_id = &self
            .open_documents
            .get(uri)
            .unwrap_or_else(|| {
                panic!("{op_name} should not be called for a document that has not been opened",)
            })
            .compilation;
        let compilation = self.compilations.get(compilation_id).unwrap_or_else(|| {
            panic!("{op_name} should not be called before compilation has been initialized",)
        });

        let res = op(compilation, uri, offset);
        trace!("{op_name} result: {res:?}");
        res
    }

    // It gets really messy knowing when to clear diagnostics
    // when the document changes ownership between compilations, etc.
    // So let's do it the simplest way possible. Refresh everything every time.
    fn publish_diagnostics(&mut self) {
        let last_docs_with_errors = take(&mut self.documents_with_errors);

        for (compilation_uri, compilation) in &self.compilations {
            trace!("publishing diagnostics for {compilation_uri}");
            for (uri, errors) in errors_by_doc(compilation_uri, &compilation.errors) {
                if !self.documents_with_errors.insert(uri.clone()) {
                    // We already published diagnostics for this document for
                    // a different compilation.
                    // When the same document is included in multiple compilations,
                    // only report the errors for one of them, the goal being
                    // a less confusing user experience.
                    continue;
                }

                self.publish_diagnostics_for_doc(&uri, errors);
            }
        }

        // Clear errors from any documents that previously had errors
        for uri in last_docs_with_errors.difference(&self.documents_with_errors) {
            self.publish_diagnostics_for_doc(uri, vec![]);
        }

        // TODO: errors without an associated span
        // let project_errors = compilation
        //     .errors
        //     .iter()
        //     .filter(|e| e.labels().into_iter().flatten().next().is_none());
    }

    fn publish_diagnostics_for_doc(&self, uri: &str, errors: Vec<Error>) {
        let version = self.open_documents.get(uri).map(|d| d.version);
        trace!("publishing diagnostics for {uri} {version:?}): {errors:?}");
        // Publish diagnostics
        (self.diagnostics_receiver)(DiagnosticUpdate {
            uri: uri.into(),
            version,
            errors,
        });
    }

    fn apply_configuration(&mut self, configuration: &WorkspaceConfigurationUpdate) -> bool {
        let mut need_recompile = false;

        if let Some(package_type) = configuration.package_type {
            need_recompile |= self.configuration.package_type != package_type;
            self.configuration.package_type = package_type;
        }

        if let Some(target_profile) = configuration.target_profile {
            need_recompile |= self.configuration.target_profile != target_profile;
            self.configuration.target_profile = target_profile;
        }

        trace!("need_recompile after configuration update: {need_recompile}");
        need_recompile
    }

    /// Recompiles the currently known documents with
    /// the current configuration. Publishes updated
    /// diagnostics for all documents.
    fn recompile_all(&mut self) {
        for compilation in self.compilations.values_mut() {
            compilation.recompile(
                self.configuration.package_type,
                self.configuration.target_profile,
            );
        }

        self.publish_diagnostics();
    }
}

fn errors_by_doc(compilation_uri: &Arc<str>, errors: &Vec<Error>) -> HashMap<Arc<str>, Vec<Error>> {
    let mut map = HashMap::new();

    for err in errors {
        // Use the compilation_uri as a location for span-less errors
        let doc = err
            .labels()
            .into_iter()
            .flatten()
            .next()
            .map_or(compilation_uri, |l| {
                let (source, _) = err.resolve_span(l.inner());
                &source.name
            });

        map.entry(doc.clone())
            .or_insert_with(Vec::new)
            .push(err.clone());
    }

    map
}
