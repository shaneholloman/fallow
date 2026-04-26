import { describe, expect, it } from "vitest";
import {
  buildParamsFromCli,
  buildStatusBarPartsFromLsp,
  buildStatusBarTooltipMarkdown,
  formatChangedSinceRefForStatusBar,
  getStatusBarSeverityKey,
  getDuplicationPercentage,
} from "../src/statusBar-utils.js";
import type { AnalysisCompleteParams } from "../src/statusBar-utils.js";
import type { FallowCheckResult, FallowDupesResult } from "../src/types.js";

const baseParams = (
  overrides: Partial<AnalysisCompleteParams> = {}
): AnalysisCompleteParams => ({
  totalIssues: 0,
  unusedFiles: 0,
  unusedExports: 0,
  unusedTypes: 0,
  unusedDependencies: 0,
  unusedDevDependencies: 0,
  unusedOptionalDependencies: 0,
  unusedEnumMembers: 0,
  unusedClassMembers: 0,
  unresolvedImports: 0,
  unlistedDependencies: 0,
  duplicateExports: 0,
  typeOnlyDependencies: 0,
  circularDependencies: 0,
  duplicationPercentage: 0,
  cloneGroups: 0,
  ...overrides,
});

describe("getDuplicationPercentage", () => {
  it("clamps non-finite values to zero", () => {
    expect(getDuplicationPercentage(Number.NaN)).toBe(0);
    expect(getDuplicationPercentage(Number.POSITIVE_INFINITY)).toBe(0);
  });

  it("keeps finite values unchanged", () => {
    expect(getDuplicationPercentage(4.25)).toBe(4.25);
  });
});

describe("buildStatusBarPartsFromLsp", () => {
  it("builds issue and duplication summary parts", () => {
    expect(
      buildStatusBarPartsFromLsp(
        baseParams({ totalIssues: 3, duplicationPercentage: 1.234 })
      )
    ).toEqual(["3 issues", "1.2% duplication"]);
  });
});

describe("getStatusBarSeverityKey", () => {
  it("prefers error styling for unresolved imports", () => {
    expect(
      getStatusBarSeverityKey(
        baseParams({ totalIssues: 2, unresolvedImports: 1 })
      )
    ).toBe("statusBarItem.errorBackground");
  });

  it("uses warning styling when issues exist without unresolved imports", () => {
    expect(
      getStatusBarSeverityKey(baseParams({ totalIssues: 2 }))
    ).toBe("statusBarItem.warningBackground");
  });

  it("returns null when there are no issues", () => {
    expect(getStatusBarSeverityKey(baseParams())).toBeNull();
  });
});

describe("buildStatusBarTooltipMarkdown", () => {
  it("includes only present issue categories and action links", () => {
    const markdown = buildStatusBarTooltipMarkdown(
      baseParams({
        totalIssues: 4,
        unusedFiles: 1,
        unresolvedImports: 2,
        cloneGroups: 1,
        duplicationPercentage: 3.25,
      })
    );

    expect(markdown).toContain("**Fallow** - Analysis Results");
    expect(markdown).toContain("$(error) 2 unresolved imports");
    expect(markdown).toContain("$(warning) 1 unused files");
    expect(markdown).toContain("$(copy) 1 clone groups (3.3% duplication)");
    expect(markdown).toContain("command:fallow.analyze");
    expect(markdown).not.toContain("unused exports");
  });

  it("shows a success message when no issues or clones exist", () => {
    const markdown = buildStatusBarTooltipMarkdown(baseParams());

    expect(markdown).toContain("$(check) No issues found");
  });

  it("surfaces the changedSince ref when scoped", () => {
    const markdown = buildStatusBarTooltipMarkdown(baseParams(), "fallow-baseline");
    expect(markdown).toContain("Scoped to changes since fallow\\-baseline");
  });

  it("escapes changedSince markdown in trusted tooltip text", () => {
    const markdown = buildStatusBarTooltipMarkdown(
      baseParams(),
      "base` [open](command:workbench.action.openSettings)"
    );
    expect(markdown).toContain(
      "base\\` \\[open\\]\\(command:workbench\\.action\\.openSettings\\)"
    );
  });

  it("omits the scope line when no changedSince ref is given", () => {
    const markdown = buildStatusBarTooltipMarkdown(baseParams());
    expect(markdown).not.toContain("Scoped to changes since");
  });
});

describe("formatChangedSinceRefForStatusBar", () => {
  it("normalizes whitespace for the compact status bar label", () => {
    expect(formatChangedSinceRefForStatusBar(" feature\nbranch\tname ")).toBe(
      "feature branch name"
    );
  });

  it("truncates long refs for the compact status bar label", () => {
    const formatted = formatChangedSinceRefForStatusBar(
      "feature/some-extremely-long-baseline-branch-name-that-would-crowd-the-status-bar"
    );
    expect(formatted.length).toBeLessThanOrEqual(48);
    expect(formatted).toMatch(/\.\.\.$/);
  });
});

describe("buildParamsFromCli", () => {
  const emptyCheck = (): FallowCheckResult => ({
    unused_files: [],
    unused_exports: [],
    unused_types: [],
    unused_dependencies: [],
    unused_dev_dependencies: [],
    unused_optional_dependencies: [],
    unused_enum_members: [],
    unused_class_members: [],
    unresolved_imports: [],
    unlisted_dependencies: [],
    duplicate_exports: [],
    type_only_dependencies: [],
    circular_dependencies: [],
  });

  it("returns zero counts when both inputs are null", () => {
    const params = buildParamsFromCli(null, null);
    expect(params.totalIssues).toBe(0);
    expect(params.duplicationPercentage).toBe(0);
    expect(params.cloneGroups).toBe(0);
  });

  it("counts issue categories from the check result", () => {
    const check: FallowCheckResult = {
      ...emptyCheck(),
      unused_files: [{ path: "a.ts" }],
      unused_exports: [
        { path: "b.ts", export_name: "x", line: 1, col: 0 },
        { path: "c.ts", export_name: "y", line: 1, col: 0 },
      ],
      unused_optional_dependencies: [
        { path: "package.json", package_name: "fsevents" },
      ],
      unresolved_imports: [
        { path: "d.ts", specifier: "./missing", line: 1, col: 0 },
      ],
    };

    const params = buildParamsFromCli(check, null);
    expect(params.unusedFiles).toBe(1);
    expect(params.unusedExports).toBe(2);
    expect(params.unusedOptionalDependencies).toBe(1);
    expect(params.unresolvedImports).toBe(1);
    expect(params.totalIssues).toBe(5);
    expect(params.duplicationPercentage).toBe(0);
  });

  it("propagates duplication stats from the dupes result so the tooltip matches the status bar text", () => {
    const dupes: FallowDupesResult = {
      clone_groups: [],
      clone_families: [],
      stats: {
        total_files: 10,
        files_with_clones: 2,
        total_lines: 1000,
        duplicated_lines: 8,
        total_tokens: 5000,
        duplicated_tokens: 40,
        clone_groups: 3,
        clone_instances: 6,
        duplication_percentage: 0.8,
      },
    };

    const params = buildParamsFromCli(null, dupes);
    expect(params.duplicationPercentage).toBe(0.8);
    expect(params.cloneGroups).toBe(3);
  });

  it("treats missing optional check fields as zero counts", () => {
    const check = emptyCheck();
    delete (check as { type_only_dependencies?: unknown })
      .type_only_dependencies;
    delete (check as { circular_dependencies?: unknown })
      .circular_dependencies;
    delete (check as { unused_optional_dependencies?: unknown })
      .unused_optional_dependencies;

    const params = buildParamsFromCli(check, null);
    expect(params.unusedOptionalDependencies).toBe(0);
    expect(params.typeOnlyDependencies).toBe(0);
    expect(params.circularDependencies).toBe(0);
  });
});
