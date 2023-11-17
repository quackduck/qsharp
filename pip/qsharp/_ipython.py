# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from IPython.display import display, Javascript, Pretty
from IPython.core.magic import register_cell_magic
from ._native import QSharpError
from ._qsharp import _get_interpreter
import pathlib


def register_magic():
    @register_cell_magic
    def qsharp(line, cell):
        """Cell magic to interpret Q# code in Jupyter notebooks."""

        def callback(output):
            display(output)

        try:
            return _get_interpreter().interpret(cell, callback)
        except QSharpError as e:
            display(Pretty(str(e)))


def enable_classic_notebook_codemirror_mode():
    """
    Registers %%qsharp cells with MIME type text/x-qsharp
    and defines a CodeMirror mode to enable syntax highlighting.
    This only works in "classic" Jupyter notebooks, not Notebook v7.
    """
    js_to_inject = open(
        pathlib.Path(__file__)
        .parent.resolve()
        .joinpath(".data", "qsharp_codemirror.js"),
        mode="r",
        encoding="utf-8",
    ).read()

    # Extend the JavaScript display helper to print nothing when used
    # in a non-browser context (i.e. IPython console)
    class JavaScriptWithPlainTextFallback(Javascript):
        def __repr__(self):
            return ""

    # This will run the JavaScript in the context of the frontend.
    display(JavaScriptWithPlainTextFallback(js_to_inject))
