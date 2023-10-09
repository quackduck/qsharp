use lazy_static::lazy_static;
use std::{collections::HashMap, path::PathBuf};

macro_rules! module_loader {
    ($sources:expr) => {{
        // fn $fn_name(path: &std::path::PathBuf) -> Result<String, crate::Error> {
        //     $sources.get(path).map(Clone::clone).ok_or(crate::Error)
        // }
        |path: &std::path::PathBuf| -> Result<String, crate::Error> {
            $sources.get(path).map(Clone::clone).ok_or(crate::Error)
        }
    }};
}

#[test]
fn no_modules() {
    lazy_static! {
        static ref sources: HashMap<PathBuf, String> = Default::default();
    }
    let load_module = module_loader!(sources);
    let deps = super::find_dependency_paths("namespace Main {}", load_module).unwrap();
    assert!(deps.is_empty());
}
#[test]
fn basic_modules() {
    lazy_static! {
        static ref sources: HashMap<PathBuf, String> = HashMap::from_iter(
            vec![("foo.qs".into(), "module bar.qs;"), ("bar.qs".into(), "")]
                .into_iter()
                .map(|(x, y)| (x, y.to_string()))
        );
    }
    let load_module = module_loader!(sources);
    let deps = super::find_dependency_paths("namespace Main {}", load_module).unwrap();
    assert_eq!(deps.len(), 2);
}
