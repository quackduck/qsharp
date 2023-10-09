use qsc_parse::{Module, ModuleOrNamespace};
use std::{collections::HashMap, path::PathBuf, sync::Arc};
#[cfg(test)]
mod tests;

struct Source {
    source: String,
    /// whether or not this module has already had its dependencies inspected
    inspected: bool,
}

impl Source {
    pub fn new(raw: String) -> Self {
        Self {
            source: raw,
            inspected: false,
        }
    }
}
#[derive(Debug)]
pub struct Error;

// TODO:
// file loader injection
// error handling
pub fn find_dependency_paths<FileLoader, E>(
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
    qsc_parse::namespaces_and_modules(source)
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
