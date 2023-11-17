// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

use miette::miette;
use pyo3::{
    exceptions::PyException,
    prelude::*,
    types::{PyDict, PyList, PyString, PyTuple},
};
use qsc::project::{DirEntry, EntryType, FileSystem};

pub(crate) fn file_system(
    py: Python,
    read_file: PyObject,
    list_directory: PyObject,
) -> impl FileSystem + '_ {
    Py {
        py,
        fs_hooks: FsHooks {
            read_file,
            list_directory,
        },
    }
}

struct FsHooks {
    read_file: PyObject,
    list_directory: PyObject,
}

#[derive(Debug)]
struct Entry {
    entry_type: EntryType,
    path: String,
    entry_name: String,
    extension: String,
}

impl DirEntry for Entry {
    type Error = pyo3::PyErr;

    fn entry_type(&self) -> Result<EntryType, Self::Error> {
        Ok(self.entry_type)
    }

    fn extension(&self) -> String {
        self.extension.clone()
    }

    fn entry_name(&self) -> String {
        self.entry_name.clone()
    }

    fn path(&self) -> PathBuf {
        PathBuf::from(&self.path)
    }
}

struct Py<'a> {
    pub py: Python<'a>,
    fs_hooks: FsHooks,
}

impl FileSystem for Py<'_> {
    type Entry = Entry;

    fn read_file(&self, path: &Path) -> miette::Result<(Arc<str>, Arc<str>)> {
        read_file(self.py, &self.fs_hooks.read_file, path).map_err(IntoDiagnostic::into_diagnostic)
    }

    fn list_directory(&self, path: &Path) -> miette::Result<Vec<Self::Entry>> {
        list_directory(self.py, &self.fs_hooks.list_directory, path)
            .map_err(IntoDiagnostic::into_diagnostic)
    }
}

fn read_file(py: Python, read_file: &PyObject, path: &Path) -> PyResult<(Arc<str>, Arc<str>)> {
    let read_file_result = read_file.call1(py, PyTuple::new(py, &[path.to_string_lossy()]))?;

    let tuple = read_file_result.downcast::<PyTuple>(py)?;

    Ok((get_tuple_string(tuple, 0)?, get_tuple_string(tuple, 1)?))
}

fn list_directory(py: Python, list_directory: &PyObject, path: &Path) -> PyResult<Vec<Entry>> {
    let list_directory_result =
        list_directory.call1(py, PyTuple::new(py, &[path.to_string_lossy()]))?;

    list_directory_result
        .downcast::<PyList>(py)?
        .into_iter()
        .map(|e| {
            let dict = e.downcast::<PyDict>()?;
            let entry_type = match get_dict_string(dict, "type")? {
                "file" => EntryType::File,
                "folder" => EntryType::Folder,
                "symlink" => EntryType::Symlink,
                _ => Err(PyException::new_err(
                    "expected valid value for `type` in list_directory result",
                ))?,
            };

            Ok(Entry {
                entry_type,
                path: get_dict_string(dict, "path")?.to_string(),
                entry_name: get_dict_string(dict, "entry_name")?.to_string(),
                extension: get_dict_string(dict, "extension")?.to_string(),
            })
        })
        .collect() // Returns all values if all Ok, or first Err
}

fn get_tuple_string(tuple: &PyTuple, index: usize) -> PyResult<Arc<str>> {
    // TODO: use conversion traits from pyo3 instead
    Ok(tuple
        .get_item(index)?
        .downcast::<PyString>()?
        .to_string()
        .into())
}

fn get_dict_string<'a>(dict: &'a PyDict, key: &'a str) -> PyResult<&'a str> {
    // TODO: use conversion traits from pyo3 instead
    match dict.get_item(key)? {
        Some(item) => Ok(item.downcast::<PyString>()?.to_str()?),
        None => Err(PyException::new_err(format!("missing key `{key}` in dict"))),
    }
}

trait IntoDiagnostic {
    fn into_diagnostic(self) -> miette::Report;
}

impl IntoDiagnostic for PyErr {
    fn into_diagnostic(self) -> miette::Report {
        // Use Debug representation with traceback
        miette!(format!("{:?}", self))
    }
}
