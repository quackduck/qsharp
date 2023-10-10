//! These tests are not intended to test filesystem implementation details. Rather, they are
//! intended to test the module parsing, lookup, and deduplication functionality of the resolver.
//! That mean it does not test things like upwards imports, symlinks, etc.

use expect_test::expect;

#[macro_use]
mod utils {
    use expect_test::Expect;
    use std::{collections::HashMap, path::PathBuf};

    pub type FileContents = String;
    pub type MockFileSystem = HashMap<PathBuf, FileContents>;

    pub fn check(mut input: Vec<(PathBuf, String)>, expect: &Expect) {
        input.sort();
        expect.assert_debug_eq(&input);
    }
    /// Helper function to create the "mock filesystem" in the format of paths and file contents
    pub fn files<K, V>(files: impl IntoIterator<Item = (K, V)>) -> MockFileSystem
    where
        K: Into<PathBuf>,
        V: Into<String>,
    {
        HashMap::from_iter(files.into_iter().map(|(x, y)| (x.into(), y.into())))
    }

    /// This macro helps us construct an in-memory file loader to inject for testing.
    /// This way we can write module resolution tests without actually touching the
    /// filesystem.
    macro_rules! module_loader {
        ($sources:expr) => {{
            |path: &std::path::PathBuf| -> Result<String, crate::Error> {
                $sources
                    .get(path)
                    .map(Clone::clone)
                    .ok_or(crate::Error::MockedFsError)
            }
        }};
    }
}

use crate::tests::utils::*;

#[test]
fn no_modules() {
    let sources: MockFileSystem = Default::default();
    let load_module = module_loader!(sources);
    let deps = super::find_dependencies_with_loader("namespace Main {}", load_module).unwrap();
    check(
        deps,
        &expect!([r#"
        []
    "#]),
    );
}

#[test]
fn basic_modules() {
    let sources = files(vec![("foo.qs", r#"module "bar.qs";"#), ("bar.qs", "")]);
    let load_module = module_loader!(sources);
    let deps = super::find_dependencies_with_loader(r#"module "foo.qs";"#, load_module).unwrap();
    check(
        deps,
        &expect!([r#"
        [
            (
                "bar.qs",
                "",
            ),
            (
                "foo.qs",
                "module \"bar.qs\";",
            ),
        ]
    "#]),
    );
}

#[test]
fn nested() {
    let sources = files(vec![
        ("foo.qs", r#"module "bar.qs"; module "bar/baz.qs";"#),
        ("bar.qs", ""),
        ("bar/baz.qs", ""),
    ]);
    let load_module = module_loader!(sources);
    let deps = super::find_dependencies_with_loader(r#"module "foo.qs";"#, load_module).unwrap();
    check(
        deps,
        &expect!([r#"
        [
            (
                "bar/baz.qs",
                "",
            ),
            (
                "bar.qs",
                "",
            ),
            (
                "foo.qs",
                "module \"bar.qs\"; module \"bar/baz.qs\";",
            ),
        ]
    "#]),
    );
}

// TODO:
// I think this behavior is actually undesirable. If we go the rust mod path, and abandon
// file paths, then this becomes a non-issue as declaring the same path from different files
// is impossible by construction.
// This test tests that we dedup when a module is declared more than once.
#[test]
fn declared_from_different_files() {
    let sources = files(vec![
        ("foo.qs", r#"module "bar.qs"; module "bar/baz.qs";"#),
        ("bar.qs", r#"module "bar/baz.qs";"#),
        ("bar/baz.qs", ""),
    ]);
    let load_module = module_loader!(sources);
    let deps = super::find_dependencies_with_loader(r#"module "foo.qs";"#, load_module).unwrap();
    check(
        deps,
        &expect!([r#"
            [
                (
                    "bar/baz.qs",
                    "",
                ),
                (
                    "bar.qs",
                    "module \"bar/baz.qs\";",
                ),
                (
                    "foo.qs",
                    "module \"bar.qs\"; module \"bar/baz.qs\";",
                ),
            ]
        "#]),
    );
}
#[test]
fn undeclared_module() {
    let sources = files(vec![("foo.qs", ""), ("bar.qs", ""), ("bar/baz.qs", "")]);
    let load_module = module_loader!(sources);
    let deps = super::find_dependencies_with_loader(r#"module "foo.qs";"#, load_module).unwrap();
    check(
        deps,
        &expect!([r#"
        [
            (
                "foo.qs",
                "",
            ),
        ]
    "#]),
    );
}
