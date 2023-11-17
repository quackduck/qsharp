# Copyright (c) Microsoft Corporation.
# Licensed under the MIT License.

from ._native import Interpreter, TargetProfile, StateDump

_interpreter = None


def init(
    *, target_profile: TargetProfile = TargetProfile.Full, project_root=None
) -> None:
    """
    Initializes the Q# interpreter.

    :param target_profile: Setting the target profile allows the Q#
        interpreter to generate programs that are compatible
        with a specific target. See :py:class: `qsharp.TargetProfile`.

    :param project_root: The root directory of the Q# project. It must
        contain a qsharp.json project manifest.
    """
    global _interpreter

    manifest_descriptor = None
    if project_root is not None:
        import os

        qsharp_json = os.path.join(project_root, "qsharp.json")
        if not os.path.exists(qsharp_json):
            raise ValueError("qsharp.json not found at project root")

        import json

        manifest_descriptor = {}
        manifest_descriptor["manifest_dir"] = project_root
        manifest_descriptor["manifest"] = json.loads(
            open(qsharp_json, mode="r", encoding="utf-8").read()
        )

    _interpreter = Interpreter(
        target_profile, manifest_descriptor, _read_file, _list_directory
    )


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

    return _get_interpreter().interpret(source, callback)


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

    return _get_interpreter().run(entry_expr, shots, callback)


def compile(entry_expr):
    """
    Compiles the Q# source code into a program that can be submitted to a target.

    :param entry_expr: The Q# expression that will be used as the entrypoint
        for the program.
    """
    ll_str = _get_interpreter().qir(entry_expr)
    return QirInputData("main", ll_str)


def dump_machine() -> StateDump:
    """
    Returns the sparse state vector of the simulator as a StateDump object.

    :returns: The state of the simulator.
    """
    return _get_interpreter().dump_machine()


def _get_interpreter() -> Interpreter:
    """
    Returns the Q# interpreter.

    :returns: The Q# interpreter.
    """
    global _interpreter
    if _interpreter is None:
        init()
    return _interpreter


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
        self._ll_str = ll_str

    # The name of this method is defined
    # by the protocol and must remain unchanged.
    def _repr_qir_(self, **kwargs) -> bytes:
        return self._ll_str.encode("utf-8")


def _read_file(path):
    f = open(path, mode="r", encoding="utf-8")
    return (path, f.read())


def _list_directory(dir_path):
    import os

    return list(
        map(
            lambda e: {
                "path": os.path.join(dir_path, e),
                "entry_name": e,
                "extension": os.path.splitext(e)[1][1:],
                "type": "file"
                if os.path.isfile(os.path.join(dir_path, e))
                else "folder"
                if os.path.isdir(os.path.join(dir_path, e))
                else "unknown",
            },
            os.listdir(dir_path),
        )
    )
