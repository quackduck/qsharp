# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._native import Interpreter, TargetProfile

import subprocess

_interpreter = None


def init(target_profile: TargetProfile = TargetProfile.Full) -> None:
    """
    Initializes the Q# interpreter.

    :param target_profile: Setting the target profile allows the Q#
        interpreter to generate programs that are compatible
        with a specific target. See :py:class: `qsharp.TargetProfile`.
    """
    global _interpreter
    _interpreter = Interpreter(target_profile)


def get_interpreter() -> Interpreter:
    """
    Returns the Q# interpreter.

    :returns: The Q# interpreter.
    """
    global _interpreter
    if _interpreter is None:
        init()
    return _interpreter


def eval(source):
    """
    Evaluates Q# source code.

    Output is printed to console.

    :param source: The Q# source code to evaluate.
    :returns value: The value returned by the last statement in the source code.
    :raises QSharpError: If there is an error evaluating the source code.
    """

    def callback(output):
        print(output)

    return get_interpreter().interpret(source, callback)


def eval_file(path):
    """
    Reads Q# source code from a file and evaluates it.

    :param path: The path to the Q# source file.
    :returns: The value returned by the last statement in the file.
    :raises: QSharpError
    """
    f = open(path, mode="r", encoding="utf-8")
    return eval(f.read())


def run(entry_expr, shots):
    """
    Runs the given Q# expressin for the given number of shots.
    Each shot uses an independent instance of the simulator.

    :param entry_expr: The entry expression.
    :param shots: The number of shots to run.

    :returns values: A list of results or runtime errors.

    :raises QSharpError: If there is an error interpreting the input.
    """

    def callback(output):
        print(output)

    return _interpreter.run(entry_expr, shots, callback)


def compile(entry_expr):
    """
    Compiles the Q# source code into a program that can be submitted to a target.

    :param entry_expr: The Q# expression that will be used as the entrypoint
        for the program.
    """
    ll_str = get_interpreter().qir(entry_expr)
    return QirInputData("main", ll_str)


# Class that wraps generated QIR, which can be used by
# azure-quantum as input data.
#
# This class must implement the QirRepresentable protocol
# that is defined by the azure-quantum package.
# See: https://github.com/microsoft/qdk-python/blob/fcd63c04aa871e49206703bbaa792329ffed13c4/azure-quantum/azure/quantum/target/target.py#L21
class QirInputData:
    # The name of this variable is defined
    # by the protocol and must remain unchanged.
    _name: str

    def __init__(self, name: str, ll_str: str):
        self._name = name
        # Write ll to file
        file = open(interim_ll_path, "w")
        file.write(ll_str)
        file.close()
        # Make .bc
        child = subprocess.Popen(
            [
                "C:\\temp\\qat.exe",
                "--apply",
                "--always-inline",
                "--no-disable-record-output-support",
                "--entry-point-attr",
                "entry_point",
                interim_ll_path,
                "C:\\src\\qsharp\\compiler\\qsc_codegen\\src\\qir_base\\decomp.ll",
                "-o",
                interim_bc_path,
            ]
        )
        if child.wait() != 0:
            raise Exception(f"Linking failed: '{child.returncode}'")
        bc_file = open(interim_bc_path, "rb")
        bc = bc_file.read()
        self.bc = bc

    # The name of this method is defined
    # by the protocol and must remain unchanged.
    def _repr_qir_(self, **kwargs) -> bytes:
        return self.bc


interim_ll_path = "C:\\temp\\interim.ll"
interim_bc_path = "C:\\temp\\interim.bc"
