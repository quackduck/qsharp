// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

/// <reference types="../../node_modules/monaco-editor/monaco.d.ts"/>

import { useEffect, useRef, useState } from "preact/hooks";
import {
  CompilerState,
  ICompilerWorker,
  QscEventTarget,
  VSDiagnostic,
  log,
} from "qsharp";
import { codeToBase64 } from "./utils.js";

type ErrCollection = {
  checkDiags: VSDiagnostic[];
  shotDiags: VSDiagnostic[];
};

function VSDiagsToMarkers(
  errors: VSDiagnostic[],
  srcModel: monaco.editor.ITextModel
): monaco.editor.IMarkerData[] {
  return errors.map((err) => {
    const startPos = srcModel.getPositionAt(err.start_pos);
    const endPos = srcModel.getPositionAt(err.end_pos);
    const marker: monaco.editor.IMarkerData = {
      severity: monaco.MarkerSeverity.Error,
      message: err.message,
      startLineNumber: startPos.lineNumber,
      startColumn: startPos.column,
      endLineNumber: endPos.lineNumber,
      endColumn: endPos.column,
    };

    return marker;
  });
}

export function Editor(props: {
  code: string;
  compiler: ICompilerWorker;
  compilerState: CompilerState;
  defaultShots: number;
  evtTarget: QscEventTarget;
  kataVerify?: string;
  onRestartCompiler: () => void;
  shotError?: VSDiagnostic;
  showExpr: boolean;
  showShots: boolean;
}) {
  const editor = useRef<monaco.editor.IStandaloneCodeEditor | null>(null);
  const errMarks = useRef<ErrCollection>({ checkDiags: [], shotDiags: [] });
  const editorDiv = useRef<HTMLDivElement>(null);

  const [shotCount, setShotCount] = useState(props.defaultShots);
  const [runExpr, setRunExpr] = useState("");
  const [errors, setErrors] = useState<{ location: string; msg: string[] }[]>(
    []
  );
  const [hasCheckErrors, setHasCheckErrors] = useState(false);

  function markErrors() {
    const model = editor.current?.getModel();
    if (!model) return;

    const errs = [
      ...errMarks.current.checkDiags,
      ...errMarks.current.shotDiags,
    ];

    const markers = VSDiagsToMarkers(errs, model);
    monaco.editor.setModelMarkers(model, "qsharp", markers);

    const errList = markers.map((err) => ({
      location: `main.qs@(${err.startLineNumber},${err.startColumn})`,
      msg: err.message.split("\\\\n\\\\n"),
    }));
    setErrors(errList);
  }

  async function onCheck() {
    const code = editor.current?.getValue();
    if (code == null) return;
    const results = await props.compiler.checkCode(code);
    errMarks.current.checkDiags = results;
    markErrors();
    setHasCheckErrors(results.length > 0);
  }

  async function onRun() {
    const code = editor.current?.getValue();
    if (code == null) return;
    props.evtTarget.clearResults();

    try {
      if (props.kataVerify) {
        // This is for a kata. Provide the verification code.
        await props.compiler.runKata(code, props.kataVerify, props.evtTarget);
      } else {
        await props.compiler.run(code, runExpr, shotCount, props.evtTarget);
      }
    } catch (err) {
      // This could fail for several reasons, e.g. the run being cancelled.
      if (err === "terminated") {
        log.info("Run was terminated");
      } else {
        log.error("Run failed with error: %o", err);
      }
    }
  }

  useEffect(() => {
    if (!editorDiv.current) return;
    const newEditor = monaco.editor.create(editorDiv.current, {
      minimap: { enabled: false },
      lineNumbersMinChars: 3,
    });

    editor.current = newEditor;
    const srcModel = monaco.editor.createModel(props.code, "qsharp");
    newEditor.setModel(srcModel);

    function onResize() {
      newEditor.layout();
    }

    // If the browser window resizes, tell the editor to update it's layout
    window.addEventListener("resize", onResize);
    return () => {
      log.info("Disposing a monaco editor");
      window.removeEventListener("resize", onResize);
      newEditor.dispose();
    };
  }, []);

  useEffect(() => {
    const theEditor = editor.current;
    if (!theEditor) return;
    theEditor.getModel()?.onDidChangeContent(onCheck);
  }, [props.compiler]);

  useEffect(() => {
    const theEditor = editor.current;
    if (!theEditor) return;

    theEditor.getModel()?.setValue(props.code);
    theEditor.revealLineNearTop(1);
    setShotCount(props.defaultShots);
    setRunExpr("");
  }, [props.code, props.defaultShots]);

  useEffect(() => {
    errMarks.current.shotDiags = props.shotError ? [props.shotError] : [];
    markErrors();
  }, [props.shotError]);

  // On reset, reload the initial code
  function onReset() {
    const theEditor = editor.current;
    if (!theEditor) return;
    theEditor.getModel()?.setValue(props.code || "");
    setShotCount(props.defaultShots);
    setRunExpr("");
  }

  function onGetLink() {
    const code = editor.current?.getModel()?.getValue();
    if (!code) return;

    const encodedCode = codeToBase64(code);
    const escapedCode = encodeURIComponent(encodedCode);

    // Get current URL without query parameters to use as the base URL
    const newUrl = `${window.location.href.split("?")[0]}?code=${escapedCode}`;
    // Copy link to clipboard and update url without reloading the page
    navigator.clipboard.writeText(newUrl);
    window.history.pushState({}, "", newUrl);
    // TODO: Alert user somehow link is on the clipboard
  }

  function shotCountChanged(e: Event) {
    const target = e.target as HTMLInputElement;
    setShotCount(parseInt(target.value) || 1);
  }

  function runExprChanged(e: Event) {
    const target = e.target as HTMLInputElement;
    setRunExpr(target.value);
  }

  return (
    <div class="editor-column">
      <div style="display: flex; justify-content: space-between; align-items: center;">
        <div class="file-name">main.qs</div>
        <div class="icon-row">
          <svg
            onClick={onGetLink}
            width="24px"
            height="24px"
            viewBox="0 0 24 24"
            fill="none"
          >
            <title>Get a link to this code</title>
            <path
              d="M14 12C14 14.2091 12.2091 16 10 16H6C3.79086 16 2 14.2091 2 12C2 9.79086 3.79086 8 6 8H8M10 12C10 9.79086 11.7909 8 14 8H18C20.2091 8 22 9.79086 22 12C22 14.2091 20.2091 16 18 16H16"
              stroke="#000000"
              stroke-width="2"
              stroke-linecap="round"
              stroke-linejoin="round"
            />
          </svg>
          <svg
            onClick={onReset}
            width="24px"
            height="24px"
            viewBox="0 0 24 24"
            fill="none"
          >
            <title>Reset code to initial state</title>
            <path
              d="M4,13 C4,17.4183 7.58172,21 12,21 C16.4183,21 20,17.4183 20,13 C20,8.58172 16.4183,5 12,5 C10.4407,5 8.98566,5.44609 7.75543,6.21762"
              stroke="#0C0310"
              stroke-width="2"
              stroke-linecap="round"
            ></path>
            <path
              d="M9.2384,1.89795 L7.49856,5.83917 C7.27552,6.34441 7.50429,6.9348 8.00954,7.15784 L11.9508,8.89768"
              stroke="#0C0310"
              stroke-width="2"
              stroke-linecap="round"
            ></path>
          </svg>
        </div>
      </div>
      <div class="code-editor" ref={editorDiv}></div>
      <div class="button-row">
        {props.showExpr ? (
          <>
            <span>Start</span>
            <input
              style="width: 160px"
              value={runExpr}
              onChange={runExprChanged}
            />
          </>
        ) : null}
        {props.showShots ? (
          <>
            <span>Shots</span>
            <input
              style="width: 88px;"
              type="number"
              value={shotCount || 100}
              max="1000"
              min="1"
              onChange={shotCountChanged}
            />
          </>
        ) : null}
        <button
          class="main-button"
          onClick={onRun}
          disabled={hasCheckErrors || props.compilerState === "busy"}
        >
          Run
        </button>
        <button
          class="main-button"
          onClick={props.onRestartCompiler}
          disabled={props.compilerState === "idle"}
        >
          Cancel
        </button>
      </div>
      <div class="error-list">
        {errors.map((err) => (
          <div class="error-row">
            <span>{err.location}: </span>
            <span>{err.msg[0]}</span>
            {err.msg.length > 1 ? (
              <div class="error-help">{err.msg[1]}</div>
            ) : null}
          </div>
        ))}
      </div>
    </div>
  );
}