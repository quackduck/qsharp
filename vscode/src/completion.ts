// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService } from "qsharp";
import * as vscode from "vscode";
import { CompletionItem } from "vscode";

export function createCompletionItemProvider(
  languageService: ILanguageService
) {
  return new QSharpCompletionItemProvider(languageService);
}

class QSharpCompletionItemProvider implements vscode.CompletionItemProvider {
  constructor(public languageService: ILanguageService) {}

  async provideCompletionItems(
    document: vscode.TextDocument,
    position: vscode.Position
  ) {
    const completions = await this.languageService.getCompletions(
      document.uri.toString(),
      position
    );
    return completions.items.map((c) => {
      let kind;
      switch (c.kind) {
        case "function":
          kind = vscode.CompletionItemKind.Function;
          break;
        case "interface":
          kind = vscode.CompletionItemKind.Interface;
          break;
        case "keyword":
          kind = vscode.CompletionItemKind.Keyword;
          break;
        case "module":
          kind = vscode.CompletionItemKind.Module;
          break;
      }
      const item = new CompletionItem(c.label, kind);
      item.sortText = c.sortText;
      item.detail = c.detail;
      item.additionalTextEdits = c.additionalTextEdits?.map((edit) => {
        return new vscode.TextEdit(
          new vscode.Range(
            new vscode.Position(
              edit.range.start.line,
              edit.range.start.character
            ),
            new vscode.Position(edit.range.end.line, edit.range.end.character)
          ),
          edit.newText
        );
      });
      return item;
    });
  }
}
