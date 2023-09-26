use std::{
    fs,
    io::{self, Read},
    path::Path,
};

use miette::{Context, IntoDiagnostic};
use qsc_ast::ast::Ident;

use super::Result;
use crate::scan::Scanner;
use qsc_fs_util::{SourceContents, SourceMap, SourceName};

/// The file name, excluding the extension `.qs`
/// that we expect to see when looking for a folder submodule.
/// That is, `mod foo;` would correspond to `foo/<MODULE_FILE_NAME>.qs`
const MODULE_FILE_NAME: &str = "Module";

pub(crate) struct Module {}

/// Loads a module file into the [Scanner]. For a given `module foo;` declaration,
/// we first look either a sibling `foo.qs` file, or `foo/Module.qs`. It is an
/// error to have _both_ files.
pub(crate) fn load_module_file(s: &mut Scanner, tok: Box<Ident>) -> Result<()> {
    let sibling_result = load_sibling_module(tok)?;
    let folder_result = load_folder_module(tok)?;

    if sibling_result.is_some() && folder_result.is_some() {
        todo!("Return error for having both module options");
    }

    todo!()
}

fn load_folder_module(tok: Box<Ident>) -> Result<Option<Module>> {
    todo!()
}

fn load_sibling_module(tok: Box<Ident>) -> Result<Option<Module>> {
    // TODO might need to track current path buf in `s`
    let file_name = format!("{MODULE_FILE_NAME}.qs");
    todo!()
}
fn read_source(path: impl AsRef<Path>) -> miette::Result<(SourceName, SourceContents)> {
    let path = path.as_ref();
    if path.as_os_str() == "-" {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .into_diagnostic()
            .context("could not read standard input")?;

        Ok(("<stdin>".into(), input.into()))
    } else {
        let contents = fs::read_to_string(path)
            .into_diagnostic()
            .with_context(|| format!("could not read source file `{}`", path.display()))?;

        Ok((path.to_string_lossy().into(), contents.into()))
    }
}
