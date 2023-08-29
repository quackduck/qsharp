// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { ILanguageService } from "qsharp";
import * as vscode from "vscode";

export function createDefinitionProvider(languageService: ILanguageService) {
  return new QSharpDefinitionProvider(languageService);
}

class QSharpDefinitionProvider implements vscode.DefinitionProvider {
  constructor(public languageService: ILanguageService) {}

  async provideDefinition(
    document: vscode.TextDocument,
    position: vscode.Position
  ) {
    const definition = await this.languageService.getDefinition(
      document.uri.toString(),
      position
    );
    if (!definition) return null;
    const uri = vscode.Uri.parse(definition.source);
    return new vscode.Location(
      uri,
      new vscode.Position(
        definition.position.line,
        definition.position.character
      )
    );
  }
}
