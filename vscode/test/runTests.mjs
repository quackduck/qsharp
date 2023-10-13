// Copyright (c) Microsoft Corporation.
// Licensed under the MIT License.

// @ts-check

// Adapted from the sample at
// https://github.com/microsoft/vscode-test-web/blob/3f0f858ab15cb65ef3c19564b0f5a6910ea9414e/sample/src/web/test/runTest.ts
//
// This script is run using Node.js in the dev environment. It will
// download the latest Insiders build of VS Code for the Web and launch
// it in a headless instance of Chromium to run the integration test suite.

import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";
import { runTests } from "./vscode-tests.mjs";

const thisDir = dirname(fileURLToPath(import.meta.url));
// The folder containing the Extension Manifest package.json
const extensionDevelopmentPath = join(thisDir, "..");
const attachArgName = "--waitForDebugger=";
const waitForDebugger = process.argv.find((arg) =>
  arg.startsWith(attachArgName)
);
const verboseArgName = "--verbose";
const verbose = process.argv.includes(verboseArgName);

try {
  // Language service tests
  await runSuite(
    join(thisDir, "out", "language-service", "index"),
    join(thisDir, "suites", "language-service", "test-workspace")
  );

  // Debugger tests
  await runSuite(
    join(thisDir, "out", "debugger", "index"),
    join(thisDir, "..", "..", "samples")
  );
} catch (err) {
  console.error("Failed to run tests", err);
  process.exit(1);
}

/**
 * @param {string} extensionTestsPath - The path to module with the test runner and tests
 * @param {string} workspacePath - The path to the workspace to be opened in VS Code
 */
async function runSuite(extensionTestsPath, workspacePath) {
  // Start a web server that serves VS Code in a browser, run the tests

  let success = { chromium: 0, firefox: 0, webkit: 0 };

  for (let i = 0; i < 5; i++) {
    console.log("::group::running tests with chromium " + i );
    try {
      await runTests({
        headless: true, // pass false to see VS Code UI
        browserType: "chromium",
        extensionDevelopmentPath,
        extensionTestsPath,
        folderPath: workspacePath,
        quality: "stable",
        printServerLog: verbose,
        verbose,
        waitForDebugger: waitForDebugger
          ? Number(waitForDebugger.slice(attachArgName.length))
          : undefined,
      });
      success.chromium++;
    } catch (e) {
      /* empty */
    }
    console.log("::endgroup::");

    console.log("::group::running tests with firefox " + i);
    try {
      await runTests({
        headless: true, // pass false to see VS Code UI
        browserType: "firefox",
        extensionDevelopmentPath,
        extensionTestsPath,
        folderPath: workspacePath,
        quality: "stable",
        printServerLog: verbose,
        verbose,
        waitForDebugger: waitForDebugger
          ? Number(waitForDebugger.slice(attachArgName.length))
          : undefined,
      });
      success.firefox++;
    } catch (e) {
      /* empty */
    }
    console.log("::endgroup::");

    // webkit hates mixed content and/or insecure http
    // console.log("::group::running tests with webkit " + i);
    // try {
    //   await runTests({
    //     headless: true, // pass false to see VS Code UI
    //     browserType: "webkit",
    //     extensionDevelopmentPath,
    //     extensionTestsPath,
    //     folderPath: workspacePath,
    //     quality: "stable",
    //     printServerLog: verbose,
    //     verbose,
    //     waitForDebugger: waitForDebugger
    //       ? Number(waitForDebugger.slice(attachArgName.length))
    //       : undefined,
    //   });
    //   success.webkit++;
    // } catch (e) {
    //   /* empty */
    // }
  }
  console.log("::endgroup::");

  console.log("success", success);
}
