mod source;

#[cfg(test)]
mod tests;

use qsc_parse::{Module, ModuleOrNamespace};
use source::Source;
use std::{collections::HashMap, fs, path::PathBuf};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    ResolverIoError(#[from] std::io::Error),
    #[error("Unable to load file from mocked fs")]
    MockedFsError,
}

pub fn find_dependency_paths(source: &str) -> Result<Vec<(PathBuf, String)>, Error> {
    find_dependencies_with_loader(source, load_module)
}

// TODO:
// error handling
fn find_dependencies_with_loader<FileLoader, E>(
    source: &str,
    load_module: FileLoader,
) -> Result<Vec<(PathBuf, String)>, Error>
where
    FileLoader: Fn(&PathBuf) -> Result<String, E>,
    E: Into<Error>,
{
    let mut parsed_sources: HashMap<PathBuf, Source> = Default::default();

    let mut initial_modules = parse_module_declarations(source, &parsed_sources);
    initial_modules.sort();
    initial_modules.dedup();

    for module in initial_modules {
        let src = load_module(&module).map_err(Into::into)?;
        parsed_sources.insert(module, Source::new(src));
    }

    loop {
        let mut new_modules = parsed_sources
            .iter()
            .filter(|(_, src)| !src.inspected)
            .flat_map(|(_, source)| parse_module_declarations(&*source.source, &parsed_sources))
            .collect::<Vec<_>>();

        new_modules.sort();
        new_modules.dedup();
        if new_modules.is_empty() {
            break;
        }

        parsed_sources
            .iter_mut()
            .for_each(|(_, src)| src.inspected = true);

        for module in new_modules {
            let src = load_module(&module).map_err(Into::into)?;
            parsed_sources.insert(module, Source::new(src));
        }
    }
    Ok(parsed_sources
        .into_iter()
        .map(|(path, source)| (path, source.source))
        .collect())
}

fn parse_module_declarations(
    source: &str,
    parsed_sources: &HashMap<PathBuf, Source>,
) -> Vec<PathBuf> {
    println!("parse_module_declarations");
    let ns_modules_res = qsc_parse::namespaces_and_modules(source);
    if !ns_modules_res.1.is_empty() {
        dbg!(&ns_modules_res.1);
        todo!("Return error here")
    };

    ns_modules_res
        .0
        .into_iter()
        .filter_map(|item| match item {
            ModuleOrNamespace::Module(Module { path }) if !parsed_sources.contains_key(&path) => {
                Some(path)
            }
            _ => None,
        })
        .collect()
}

// This is where we implement file loading semantics. If we had a manifest file or notion of a project root,
// this is where we could construct a module tree and locate files
fn load_module(path: &PathBuf) -> Result<String, std::io::Error> {
    fs::read_to_string(path)
}
