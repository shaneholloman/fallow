def count(obj; key): obj | if . then .[key] // 0 else 0 end;
def pct(n): n | . * 10 | round / 10;
def rel_path: split("/") | if length > 3 then .[-3:] | join("/") else join("/") end;
def docs(anchor): "https://docs.fallow.tools/explanations/dead-code#" + anchor;
def health_docs: "https://docs.fallow.tools/explanations/health";
def dupes_docs: "https://docs.fallow.tools/explanations/duplication";

(count(.check; "total_issues")) as $check |
(count(.dupes.stats; "clone_groups")) as $dupes |
(count(.health.summary; "functions_above_threshold")) as $health |
($check + $dupes + $health) as $total |
(.health.vital_signs // {}) as $vitals |
(.health.summary // {}) as $summary |
(.dupes.stats // {}) as $dupes_stats |

if $total == 0 then
  "# \ud83c\udf3f Fallow\n\n" +
  "> [!NOTE]\n> **No issues found**\n\n" +
  ":white_check_mark: No code issues \u00b7 :white_check_mark: No duplication \u00b7 :white_check_mark: No complex functions" +
  (if $vitals.maintainability_avg then
    "\n\n| Metric | Value |\n|:-------|------:|\n" +
    "| [Maintainability](\(health_docs)) | **\(pct($vitals.maintainability_avg))** / 100 |\n"
  else "" end)
else
  "# \ud83c\udf3f Fallow\n\n" +

  # One-line status
  (if $check > 0 then ":warning: **\($check)** code issues" else ":white_check_mark: No code issues" end) +
  " \u00b7 " +
  (if $dupes > 0 then ":warning: **\($dupes)** clone groups" else ":white_check_mark: No duplication" end) +
  " \u00b7 " +
  (if $health > 0 then ":warning: **\($health)** complex functions" else ":white_check_mark: No complex functions" end) +
  "\n\n" +

  # Pointer to inline comments
  (if $check > 0 or $dupes > 0 or $health > 0 then
    "See inline review comments for per-finding details.\n\n"
  else "" end) +

  # Code issues breakdown
  (if $check > 0 then
    "<details>\n<summary><strong>Code issues (\($check))</strong></summary>\n\n" +
    "| Category | Count |\n|:---------|------:|\n" +
    ([
      (if (.check.unused_files | length) > 0 then "| [Unused files](\(docs("unused-files"))) | \(.check.unused_files | length) |" else null end),
      (if (.check.unused_exports | length) > 0 then "| [Unused exports](\(docs("unused-exports"))) | \(.check.unused_exports | length) |" else null end),
      (if (.check.unused_types | length) > 0 then "| [Unused types](\(docs("unused-types"))) | \(.check.unused_types | length) |" else null end),
      (if (.check.unused_dependencies | length) > 0 then "| [Unused dependencies](\(docs("unused-dependencies"))) | \(.check.unused_dependencies | length) |" else null end),
      (if (.check.unused_dev_dependencies | length) > 0 then "| [Unused devDependencies](\(docs("unused-dependencies"))) | \(.check.unused_dev_dependencies | length) |" else null end),
      (if (.check.unused_optional_dependencies | length) > 0 then "| [Unused optionalDependencies](\(docs("unused-dependencies"))) | \(.check.unused_optional_dependencies | length) |" else null end),
      (if (.check.unused_enum_members | length) > 0 then "| [Unused enum members](\(docs("unused-enum-members"))) | \(.check.unused_enum_members | length) |" else null end),
      (if (.check.unused_class_members | length) > 0 then "| [Unused class members](\(docs("unused-class-members"))) | \(.check.unused_class_members | length) |" else null end),
      (if (.check.unresolved_imports | length) > 0 then "| [Unresolved imports](\(docs("unresolved-imports"))) | \(.check.unresolved_imports | length) |" else null end),
      (if (.check.unlisted_dependencies | length) > 0 then "| [Unlisted dependencies](\(docs("unlisted-dependencies"))) | \(.check.unlisted_dependencies | length) |" else null end),
      (if (.check.duplicate_exports | length) > 0 then "| [Duplicate exports](\(docs("duplicate-exports"))) | \(.check.duplicate_exports | length) |" else null end),
      (if (.check.circular_dependencies | length) > 0 then "| [Circular dependencies](\(docs("circular-dependencies"))) | \(.check.circular_dependencies | length) |" else null end),
      (if (.check.boundary_violations | length) > 0 then "| [Boundary violations](\(docs("boundary-violations"))) | \(.check.boundary_violations | length) |" else null end),
      (if (.check.type_only_dependencies | length) > 0 then "| [Type-only dependencies](\(docs("type-only-dependencies"))) | \(.check.type_only_dependencies | length) |" else null end),
      (if (.check.test_only_dependencies | length) > 0 then "| [Test-only dependencies](\(docs("test-only-dependencies"))) | \(.check.test_only_dependencies | length) |" else null end)
    ] | map(select(. != null)) | join("\n")) +
    "\n\n</details>\n\n"
  else "" end) +

  # Duplication breakdown
  (if $dupes > 0 then
    "<details>\n<summary><strong><a href=\"\(dupes_docs)\">Duplication</a> (\($dupes) clone groups, \(pct($dupes_stats.duplication_percentage))%)</strong></summary>\n\n" +
    "| Metric | Value |\n|:-------|------:|\n" +
    "| Duplicated lines | \($dupes_stats.duplicated_lines) |\n" +
    "| Clone instances | \($dupes_stats.clone_instances) |\n" +
    "| Files with clones | \($dupes_stats.files_with_clones) |\n" +
    "\n</details>\n\n"
  else "" end) +

  # Complexity breakdown
  (if $health > 0 then
    "<details>\n<summary><strong>Complexity (\($health) functions above threshold)</strong></summary>\n\n" +
    "| File | Function | Cyclomatic | Cognitive |\n|:-----|:---------|----------:|---------:|\n" +
    ([.health.findings[:5][] |
      "| `\(.path | rel_path):\(.line)` | `\(.name)` | \(.cyclomatic) | \(.cognitive) |"
    ] | join("\n")) +
    "\n\n</details>\n\n"
  else "" end) +

  # Vital signs
  (if $vitals | length > 0 then
    # Compute scoped maintainability from filtered file_scores (differs from codebase avg when --changed-since is active)
    ((.health.file_scores // []) | if length > 0 then (map(.maintainability_index) | add / length | . * 10 | round / 10) else null end) as $scoped_maint |
    "#### Codebase health\n\n" +
    "| Metric | Value |\n|:-------|------:|\n" +
    (if $vitals.maintainability_avg then "| [Maintainability](\(health_docs)) | **\(pct($vitals.maintainability_avg))** / 100 |\n" else "" end) +
    (if $scoped_maint != null and $scoped_maint != pct($vitals.maintainability_avg // 0) then
      "| [Maintainability](\(health_docs)) (changed files) | **\($scoped_maint)** / 100 |\n"
    else "" end) +
    (if $vitals.avg_cyclomatic then "| Avg complexity | \(pct($vitals.avg_cyclomatic)) |\n" else "" end) +
    "\n"
  else "" end) +

  # Conditional tips based on which categories were found
  (if ((.check.unused_exports // []) + (.check.unused_dependencies // []) + (.check.unused_enum_members // [])) | length > 0 then
    "> [!TIP]\n" +
    "> Run `fallow fix --dry-run` to preview auto-fixes.\n" +
    (if (.check.unused_exports // []) | length > 0 then
      "> Add [`/** @public */`](https://docs.fallow.tools/configuration/suppression) above exports to preserve them.\n"
    else "" end)
  else "" end)
end
