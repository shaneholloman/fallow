import * as fs from "node:fs";
// VS Code injects this module into the extension host at runtime.
// fallow-ignore-next-line unlisted-dependency
import * as vscode from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
} from "vscode-languageclient/node.js";
import { Trace } from "vscode-languageserver-protocol";
import { getLspPath, getTraceLevel, getAutoDownload, getIssueTypes } from "./config.js";
import { findBinaryInPath, findLocalBinary } from "./binary-utils.js";
import {
  downloadBinary,
  getInstalledBinaryPath,
} from "./download.js";

let client: LanguageClient | null = null;

const resolveBinaryPath = async (
  context: vscode.ExtensionContext,
  outputChannel?: vscode.OutputChannel
): Promise<string | null> => {
  const local = findLocalBinary("fallow-lsp");
  if (local) {
    return local;
  }

  const configPath = getLspPath();
  if (configPath) {
    if (fs.existsSync(configPath)) {
      return configPath;
    }
    void vscode.window.showWarningMessage(
      `Fallow: configured LSP path "${configPath}" does not exist.`
    );
    return null;
  }

  const inPath = findBinaryInPath("fallow-lsp");
  if (inPath) {
    return inPath;
  }

  const installed = getInstalledBinaryPath(context, outputChannel);
  if (installed) {
    return installed;
  }

  if (getAutoDownload()) {
    return downloadBinary(context);
  }

  const choice = await vscode.window.showErrorMessage(
    "Fallow: fallow-lsp binary not found. Would you like to download it?",
    "Download",
    "Set Path",
    "Cancel"
  );

  if (choice === "Download") {
    return downloadBinary(context);
  }

  if (choice === "Set Path") {
    void vscode.commands.executeCommand(
      "workbench.action.openSettings",
      "fallow.lspPath"
    );
  }

  return null;
};

export const startClient = async (
  context: vscode.ExtensionContext,
  outputChannel: vscode.OutputChannel
): Promise<LanguageClient | null> => {
  const binaryPath = await resolveBinaryPath(context, outputChannel);
  if (!binaryPath) {
    return null;
  }

  outputChannel.appendLine(`Using fallow-lsp binary: ${binaryPath}`);

  const serverOptions: ServerOptions = {
    command: binaryPath,
    transport: TransportKind.stdio,
  };

  const traceLevel = getTraceLevel();

  const clientOptions: LanguageClientOptions = {
    documentSelector: [
      { scheme: "file", language: "javascript" },
      { scheme: "file", language: "javascriptreact" },
      { scheme: "file", language: "typescript" },
      { scheme: "file", language: "typescriptreact" },
      { scheme: "file", language: "vue" },
      { scheme: "file", language: "svelte" },
      { scheme: "file", language: "astro" },
      { scheme: "file", language: "mdx" },
      { scheme: "file", language: "json" },
    ],
    outputChannel,
    traceOutputChannel: outputChannel,
    initializationOptions: {
      issueTypes: getIssueTypes(),
    },
  };

  client = new LanguageClient(
    "fallow",
    "Fallow Language Server",
    serverOptions,
    clientOptions
  );

  if (traceLevel !== "off") {
    void client.setTrace(
      traceLevel === "verbose" ? Trace.Verbose : Trace.Messages
    );
  }

  try {
    await client.start();
    outputChannel.appendLine("Fallow language server started.");
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    outputChannel.appendLine(`Failed to start language server: ${message}`);
    void vscode.window.showErrorMessage(
      `Fallow: failed to start language server. Check the output channel for details.`
    );
    client = null;
    return null;
  }

  return client;
};

export const stopClient = async (): Promise<void> => {
  if (client) {
    await client.stop();
    client = null;
  }
};

export const restartClient = async (
  context: vscode.ExtensionContext,
  outputChannel: vscode.OutputChannel
): Promise<LanguageClient | null> => {
  await stopClient();
  return startClient(context, outputChannel);
};
