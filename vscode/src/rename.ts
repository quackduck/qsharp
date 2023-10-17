// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService } from "qsharp-lang";
import * as vscode from "vscode";

export function createRenameProvider(languageService: ILanguageService) {
  return new QSharpRenameProvider(languageService);
}

class QSharpRenameProvider implements vscode.RenameProvider {
  constructor(public languageService: ILanguageService) {}

  async provideRenameEdits(
    document: vscode.TextDocument,
    position: vscode.Position,
    newName: string,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    token: vscode.CancellationToken,
  ) {
    const rename = await this.languageService.getRename(
      document.uri.toString(),
      document.offsetAt(position),
      newName,
    );
    if (!rename) return null;

    const workspaceEdit = new vscode.WorkspaceEdit();

    for (const [uri, edits] of rename.changes) {
      const vsEdits = edits.map((edit) => {
        return new vscode.TextEdit(
          new vscode.Range(
            document.positionAt(edit.range.start),
            document.positionAt(edit.range.end),
          ),
          edit.newText,
        );
      });
      workspaceEdit.set(vscode.Uri.parse(uri, true), vsEdits);
    }

    return workspaceEdit;
  }

  async prepareRename(
    document: vscode.TextDocument,
    position: vscode.Position,
    // eslint-disable-next-line @typescript-eslint/no-unused-vars
    token: vscode.CancellationToken,
  ) {
    const prepareRename = await this.languageService.prepareRename(
      document.uri.toString(),
      document.offsetAt(position),
    );
    if (prepareRename) {
      return {
        range: new vscode.Range(
          document.positionAt(prepareRename.range.start),
          document.positionAt(prepareRename.range.end),
        ),
        placeholder: prepareRename.newText,
      };
    } else {
      throw "Rename is unavailable at this location.";
    }
  }
}
