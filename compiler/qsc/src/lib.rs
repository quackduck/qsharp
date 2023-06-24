// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

#![warn(clippy::mod_module_files, clippy::pedantic, clippy::unwrap_used)]

pub mod compile;
mod error;
pub mod interpret;

pub mod ast {
    pub use qsc_ast::ast::Package;
}

pub mod resolve {
    pub use qsc_frontend::resolve::Names;
}

pub mod typeck {
    pub use qsc_frontend::typeck::Table;
}

pub use qsc_frontend::compile::{PackageStore, SourceContents, SourceMap, SourceName};

pub mod hir {
    pub use qsc_hir::{hir::*, *};
}

pub use qsc_data_structures::span::Span;
