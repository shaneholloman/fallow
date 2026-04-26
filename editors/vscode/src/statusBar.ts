// VS Code injects this module into the extension host at runtime.
// fallow-ignore-next-line unlisted-dependency
import * as vscode from "vscode";
import { getChangedSince } from "./config.js";
import {
  buildParamsFromCli,
  buildStatusBarPartsFromLsp,
  buildStatusBarTooltipMarkdown,
  formatChangedSinceRefForStatusBar,
  getStatusBarSeverityKey,
} from "./statusBar-utils.js";
import type { FallowCheckResult, FallowDupesResult } from "./types.js";
export type { AnalysisCompleteParams } from "./statusBar-utils.js";
import type { AnalysisCompleteParams } from "./statusBar-utils.js";

let statusBarItem: vscode.StatusBarItem | null = null;

export const createStatusBar = (): vscode.StatusBarItem => {
  statusBarItem = vscode.window.createStatusBarItem(
    vscode.StatusBarAlignment.Left,
    50
  );
  statusBarItem.command = "fallow.analyze";
  statusBarItem.text = "$(search) Fallow";
  statusBarItem.show();
  return statusBarItem;
};

/** Update the status bar from CLI-driven analysis results. */
export const updateStatusBar = (
  checkResult: FallowCheckResult | null,
  dupesResult: FallowDupesResult | null
): void => {
  if (!statusBarItem) {
    return;
  }

  const params = buildParamsFromCli(checkResult, dupesResult);
  applyTooltipAndSeverity(params);

  const parts: string[] = [];
  if (checkResult) {
    parts.push(`${params.totalIssues} issues`);
  }
  if (dupesResult) {
    parts.push(`${params.duplicationPercentage.toFixed(1)}% duplication`);
  }
  applyStatusBarText(parts);
};

/** Update the status bar from LSP notification data. */
export const updateStatusBarFromLsp = (params: AnalysisCompleteParams): void => {
  if (!statusBarItem) {
    return;
  }

  applyTooltipAndSeverity(params);
  applyStatusBarText(buildStatusBarPartsFromLsp(params));
};

const applyTooltipAndSeverity = (params: AnalysisCompleteParams): void => {
  if (!statusBarItem) {
    return;
  }

  const severity = getStatusBarSeverityKey(params);
  statusBarItem.backgroundColor = severity
    ? new vscode.ThemeColor(severity)
    : undefined;

  const tooltip = new vscode.MarkdownString(
    buildStatusBarTooltipMarkdown(params, getChangedSince() || null)
  );
  tooltip.isTrusted = true;
  // Required so `$(name)` codicons in the markdown render as icons rather
  // than literal text. Without this the popup shows raw `$(error)`,
  // `$(warning)`, etc. (issue #179).
  tooltip.supportThemeIcons = true;
  statusBarItem.tooltip = tooltip;
};

const applyStatusBarText = (parts: string[]): void => {
  if (!statusBarItem) {
    return;
  }
  const changedSince = getChangedSince();
  const suffix = changedSince
    ? ` (since ${formatChangedSinceRefForStatusBar(changedSince)})`
    : "";
  if (parts.length > 0) {
    statusBarItem.text = `$(search) Fallow: ${parts.join(" | ")}${suffix}`;
  } else {
    statusBarItem.text = `$(search) Fallow${suffix}`;
  }
};

export const setStatusBarAnalyzing = (): void => {
  if (statusBarItem) {
    statusBarItem.text = "$(loading~spin) Fallow: Analyzing...";
  }
};

export const setStatusBarError = (): void => {
  if (statusBarItem) {
    statusBarItem.text = "$(error) Fallow: Error";
  }
};

export const disposeStatusBar = (): void => {
  if (statusBarItem) {
    statusBarItem.dispose();
    statusBarItem = null;
  }
};
