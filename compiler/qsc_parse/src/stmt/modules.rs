use std::{
    fs,
    io::{self, Read},
    path::Path,
    sync::Arc,
};

use miette::{Context, IntoDiagnostic};
use qsc_ast::ast::Ident;

use super::Result;
use crate::{scan::Scanner, Error, ErrorKind};
use qsc_fs_util::{SourceContents, SourceMap, SourceName};

/// The file name, excluding the extension `.qs`
/// that we expect to see when looking for a folder submodule.
/// That is, `mod foo;` would correspond to `foo/<MODULE_FILE_NAME>.qs`
const MODULE_FILE_NAME: &str = "Module";

pub(crate) struct Module {}

/// Loads a module file into the [Scanner]. For a given `module foo;` declaration,
/// we first look either a sibling `foo.qs` file, or `foo/Module.qs`. It is an
/// error to have _both_ files.
pub(crate) fn load_module_file(s: &mut Scanner, tok: &Ident) -> Result<()> {
    // first, we check for the existence of the two possibilities.
    // We do this separately to disambiguate between failure to load
    // the files and them not existing.
    let module_name = tok.name.clone();

    let sibling_name = format!("{module_name}.qs");
    let folder_name = format!("{module_name}/{MODULE_FILE_NAME}.qs");

    let sibling_path = Path::new(&sibling_name);
    let folder_path = Path::new(&folder_name);

    let module_source = match (sibling_path.exists(), folder_path.exists()) {
        (true, true) => {
            todo!("Return error for having both module options");
        }
        (true, _) => read_source(sibling_path),
        (_, true) => read_source(folder_path),
        (_, _) => todo!("No corresponding module found error"),
    }
    .map_err(|_| Error(ErrorKind::FailedToLoadModule(tok.span)))?;

    // TODO do we have to retain file names?
    s.push_module(module_source.1);

    todo!("push module to scanner")
}

// TODO this is copy-pasta from qsc.rs, dedup later
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
