import * as child_process from "node:child_process";
import * as fs from "node:fs";
import * as path from "node:path";
// VS Code injects this module into the extension host at runtime.
// fallow-ignore-next-line unlisted-dependency
import * as vscode from "vscode";
import { getLspPath, getProduction, getDuplicationMode, getDuplicationThreshold, getIssueTypes } from "./config.js";
import { findBinaryInPath, findLocalBinary, getExecutableExtension } from "./binary-utils.js";
import { getInstalledBinaryPath } from "./download.js";
import {
  buildFixArgs,
  createFixPreviewItems,
  resolveFixLocation,
} from "./fix-utils.js";
import type {
  FallowCheckResult,
  FallowDupesResult,
  FallowFixResult,
  FixAction,
} from "./types.js";

const findCliBinary = (context: vscode.ExtensionContext): string | null => {
  const local = findLocalBinary("fallow");
  if (local) {
    return local;
  }

  const lspPath = getLspPath();
  if (lspPath) {
    const dir = path.dirname(lspPath);
    const cliPath = path.join(dir, `fallow${getExecutableExtension()}`);
    if (fs.existsSync(cliPath)) {
      return cliPath;
    }
  }

  const inPath = findBinaryInPath("fallow");
  if (inPath) {
    return inPath;
  }

  const installed = getInstalledBinaryPath(context);
  if (installed) {
    const dir = path.dirname(installed);
    const cliPath = path.join(dir, `fallow${getExecutableExtension()}`);
    if (fs.existsSync(cliPath)) {
      return cliPath;
    }
  }

  return null;
};

const execFallow = (
  context: vscode.ExtensionContext,
  args: ReadonlyArray<string>,
  cwd: string
): Promise<string> =>
  new Promise((resolve, reject) => {
    const binary = findCliBinary(context);
    if (!binary) {
      reject(new Error("fallow CLI binary not found in PATH."));
      return;
    }

    // Using execFile (not exec) to avoid shell injection
    child_process.execFile(
      binary,
      [...args],
      { cwd, maxBuffer: 50 * 1024 * 1024 },
      (error, stdout, stderr) => {
        if (error) {
          // Exit code 1 means issues found (expected), only reject on real errors.
          // child_process.ExecException.code is the numeric exit code.
          const exitCode = (error as child_process.ExecException).code;
          if (exitCode !== 1) {
            reject(new Error(stderr || error.message));
            return;
          }
        }
        resolve(stdout);
      }
    );
  });

/** Filter check results based on the user's issueTypes configuration. */
const filterCheckResult = (result: FallowCheckResult): FallowCheckResult => {
  const types = getIssueTypes();
  return {
    unused_files: types["unused-files"] ? result.unused_files : [],
    unused_exports: types["unused-exports"] ? result.unused_exports : [],
    unused_types: types["unused-types"] ? result.unused_types : [],
    unused_dependencies: types["unused-dependencies"] ? result.unused_dependencies : [],
    unused_dev_dependencies: types["unused-dev-dependencies"] ? result.unused_dev_dependencies : [],
    unused_enum_members: types["unused-enum-members"] ? result.unused_enum_members : [],
    unused_class_members: types["unused-class-members"] ? result.unused_class_members : [],
    unresolved_imports: types["unresolved-imports"] ? result.unresolved_imports : [],
    unlisted_dependencies: types["unlisted-dependencies"] ? result.unlisted_dependencies : [],
    duplicate_exports: types["duplicate-exports"] ? result.duplicate_exports : [],
    type_only_dependencies: types["type-only-dependencies"] ? result.type_only_dependencies : [],
    circular_dependencies: types["circular-dependencies"] ? result.circular_dependencies : [],
  };
};

const getWorkspaceRoot = (): string | null => {
  const folders = vscode.workspace.workspaceFolders;
  if (!folders || folders.length === 0) {
    return null;
  }
  return folders[0].uri.fsPath;
};

interface FixQuickPickItem extends vscode.QuickPickItem {
  readonly action: "navigate" | "apply-all";
  readonly fix?: FixAction;
}

const confirmApplyFixes = async (): Promise<boolean> => {
  const confirm = await vscode.window.showWarningMessage(
    "Fallow: This will unexport unused exports (keeps the code) and remove unused dependencies from package.json. Continue?",
    "Yes",
    "No"
  );

  return confirm === "Yes";
};

const openFixLocation = async (
  root: string,
  fix: FixAction | undefined
): Promise<void> => {
  if (!fix) {
    return;
  }

  const location = resolveFixLocation(root, fix);
  if (!location) {
    return;
  }

  await vscode.window.showTextDocument(vscode.Uri.file(location.absolutePath), {
    selection: new vscode.Range(location.line, 0, location.line, 0),
  });
};

const showDryRunPreview = async (
  root: string,
  result: FallowFixResult
): Promise<void> => {
  if (result.fixes.length === 0) {
    void vscode.window.showInformationMessage("Fallow: no fixes available.");
    return;
  }

  const quickPickItems: FixQuickPickItem[] = [];
  for (const item of createFixPreviewItems(result.fixes)) {
    if (item.action === "apply-all") {
      quickPickItems.push({
        label: "",
        kind: vscode.QuickPickItemKind.Separator,
        action: "navigate",
      });
      quickPickItems.push({
        label: "$(play) Apply all fixes",
        description: item.description,
        action: item.action,
      });
      continue;
    }

    quickPickItems.push({
      label: `$(wrench) ${item.label}`,
      description: item.description,
      detail: item.detail,
      action: item.action,
      fix: item.fix,
    });
  }

  const picked = await vscode.window.showQuickPick(quickPickItems, {
    title: `Fallow: ${result.fixes.length} fix${result.fixes.length === 1 ? "" : "es"} available`,
    placeHolder:
      "Review fixes — select 'Apply all fixes' to apply, or click a fix to navigate",
  });

  if (!picked) {
    return;
  }

  if (picked.action === "apply-all") {
    void vscode.commands.executeCommand("fallow.fix");
    return;
  }

  await openFixLocation(root, picked.fix);
};

export const runAnalysis = async (
  context: vscode.ExtensionContext
): Promise<{
  check: FallowCheckResult | null;
  dupes: FallowDupesResult | null;
}> => {
  const root = getWorkspaceRoot();
  if (!root) {
    void vscode.window.showWarningMessage("Fallow: no workspace folder open.");
    return { check: null, dupes: null };
  }

  let check: FallowCheckResult | null = null;
  let dupes: FallowDupesResult | null = null;

  try {
    const checkArgs = ["dead-code", "--format", "json", "--quiet"];
    if (getProduction()) {
      checkArgs.push("--production");
    }

    const dupesArgs = ["dupes", "--format", "json", "--quiet"];
    dupesArgs.push("--mode", getDuplicationMode());
    dupesArgs.push("--threshold", String(getDuplicationThreshold()));

    const [checkOutput, dupesOutput] = await Promise.all([
      execFallow(context, checkArgs, root),
      execFallow(context, dupesArgs, root),
    ]);

    try {
      check = filterCheckResult(JSON.parse(checkOutput) as FallowCheckResult);
    } catch {
      // Check output may be empty or non-JSON on error
    }

    try {
      dupes = JSON.parse(dupesOutput) as FallowDupesResult;
    } catch {
      // Dupes output may be empty or non-JSON on error
    }
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    void vscode.window.showErrorMessage(`Fallow analysis failed: ${message}`);
  }

  return { check, dupes };
};

export const runFix = async (
  context: vscode.ExtensionContext,
  dryRun: boolean
): Promise<FallowFixResult | null> => {
  const root = getWorkspaceRoot();
  if (!root) {
    void vscode.window.showWarningMessage("Fallow: no workspace folder open.");
    return null;
  }

  if (!dryRun && !(await confirmApplyFixes())) {
    return null;
  }

  try {
    const output = await execFallow(
      context,
      buildFixArgs(dryRun, getProduction()),
      root
    );
    const result = JSON.parse(output) as FallowFixResult;

    if (dryRun) {
      await showDryRunPreview(root, result);
    } else {
      const fixCount = result.fixes.length;
      void vscode.window.showInformationMessage(
        `Fallow: applied ${fixCount} fix${fixCount === 1 ? "" : "es"}.`
      );
    }

    return result;
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    void vscode.window.showErrorMessage(`Fallow fix failed: ${message}`);
    return null;
  }
};
