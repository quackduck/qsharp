// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import type {
  IDiagnostic,
  ICompletionList,
  IHover,
  IDefinition,
  LanguageService,
  IPosition,
} from "../../lib/node/qsc_wasm.cjs";
import { log } from "../log.js";
import { VSDiagnostic, mapDiagnostics } from "../vsdiagnostic.js";
import { IServiceEventTarget, IServiceProxy } from "../worker-proxy.js";
type QscWasm = typeof import("../../lib/node/qsc_wasm.cjs");

// Only one event type for now
export type LanguageServiceEvent = {
  type: "diagnostics";
  detail: {
    uri: string;
    version: number;
    diagnostics: VSDiagnostic[];
  };
};

// These need to be async/promise results for when communicating across a WebWorker, however
// for running the compiler in the same thread the result will be synchronous (a resolved promise).
export interface ILanguageService {
  updateDocument(
    uri: string,
    version: number,
    code: string,
    isExe: boolean
  ): Promise<void>;
  closeDocument(uri: string): Promise<void>;
  getCompletions(
    documentUri: string,
    position: IPosition
  ): Promise<ICompletionList>;
  getHover(documentUri: string, position: IPosition): Promise<IHover | null>;
  getDefinition(
    documentUri: string,
    position: IPosition
  ): Promise<IDefinition | null>;
  dispose(): Promise<void>;

  addEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void
  ): void;

  removeEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void
  ): void;
}

export const qsharpLibraryUriScheme = "qsharp-library-source";

export type ILanguageServiceWorker = ILanguageService & IServiceProxy;

export class QSharpLanguageService implements ILanguageService {
  private languageService: LanguageService;
  private eventHandler =
    new EventTarget() as IServiceEventTarget<LanguageServiceEvent>;

  // We need to keep a copy of the code for mapping diagnostics to utf16 offsets
  private code: { [uri: string]: string } = {};

  constructor(wasm: QscWasm) {
    log.info("Constructing a QSharpLanguageService instance");
    this.languageService = new wasm.LanguageService(
      this.onDiagnostics.bind(this)
    );
  }

  async updateDocument(
    documentUri: string,
    version: number,
    code: string,
    isExe: boolean
  ): Promise<void> {
    this.code[documentUri] = code;
    this.languageService.update_document(documentUri, version, code, isExe);
  }

  async closeDocument(documentUri: string): Promise<void> {
    delete this.code[documentUri];
    this.languageService.close_document(documentUri);
  }

  async getCompletions(
    documentUri: string,
    position: IPosition
  ): Promise<ICompletionList> {
    const result = this.languageService.get_completions(
      documentUri,
      position
    ) as ICompletionList;
    return result;
  }

  async getHover(
    documentUri: string,
    position: IPosition
  ): Promise<IHover | null> {
    const result = this.languageService.get_hover(
      documentUri,
      position
    ) as IHover | null;
    return result;
  }

  async getDefinition(
    documentUri: string,
    position: IPosition
  ): Promise<IDefinition | null> {
    return this.languageService.get_definition(
      documentUri,
      position
    ) as IDefinition | null;
  }

  async dispose() {
    this.languageService.free();
  }

  addEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void
  ) {
    this.eventHandler.addEventListener(type, listener);
  }

  removeEventListener<T extends LanguageServiceEvent["type"]>(
    type: T,
    listener: (event: Extract<LanguageServiceEvent, { type: T }>) => void
  ) {
    this.eventHandler.removeEventListener(type, listener);
  }

  onDiagnostics(uri: string, version: number, diagnostics: IDiagnostic[]) {
    try {
      const code = this.code[uri];
      const event = new Event("diagnostics") as LanguageServiceEvent & Event;
      event.detail = {
        uri,
        version,
        diagnostics: mapDiagnostics(diagnostics, code),
      };
      this.eventHandler.dispatchEvent(event);
    } catch (e) {
      log.error("Error in onDiagnostics", e);
    }
  }
}
