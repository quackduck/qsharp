// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

import { runMochaTests } from "../run";

export function run(): Promise<void> {
  return runMochaTests(() => {
    // We can't use any wildcards or dynamically discovered
    // paths here since ESBuild needs these modules to be
    // real paths on disk at bundling time.
    require("./language-service.test");
    require("./notebook.test");
  });
}
