# GitLab variant of summary-check.jq
# Differences from GitHub: no > [!NOTE] / > [!WARNING] / > [!TIP] callouts

def table_row(name; key):
  (.[key] | length) as $n |
  if $n > 0 then "| \(name) | \($n) |" else empty end;

def section(name; key; header; fmt):
  (.[key] | length) as $n |
  if $n > 0 then
    "\n<details><summary><strong>\(name) (\($n))</strong></summary>\n\n" +
    header +
    ([.[key][:25][] | fmt] | join("\n")) +
    (if $n > 25 then "\n\n> \($n - 25) more \u2014 run `fallow` locally for the full list" else "" end) +
    "\n\n</details>\n"
  else "" end;

if .total_issues == 0 then
  "# Fallow Dead Code Analysis\n\n" +
  "> **No issues found** \u00b7 \(.elapsed_ms)ms\n\n" +
  "All exports are used, all dependencies are declared, and no dead code was detected."
else
  "# Fallow Dead Code Analysis\n\n" +
  "> :warning: **\(.total_issues) issues** found \u00b7 \(.elapsed_ms)ms\n\n" +
  "| Category | Count |\n|----------|------:|\n" +
  ([
    table_row("Unused files"; "unused_files"),
    table_row("Unused exports"; "unused_exports"),
    table_row("Unused types"; "unused_types"),
    table_row("Unused dependencies"; "unused_dependencies"),
    table_row("Unused devDependencies"; "unused_dev_dependencies"),
    table_row("Unused optionalDependencies"; "unused_optional_dependencies"),
    table_row("Unused enum members"; "unused_enum_members"),
    table_row("Unused class members"; "unused_class_members"),
    table_row("Unresolved imports"; "unresolved_imports"),
    table_row("Unlisted dependencies"; "unlisted_dependencies"),
    table_row("Duplicate exports"; "duplicate_exports"),
    table_row("Circular dependencies"; "circular_dependencies"),
    table_row("Type-only dependencies"; "type_only_dependencies")
  ] | join("\n")) +
  "\n\n---\n" +
  section("Unused files"; "unused_files";
    "Files not reachable from any entry point.\n\n| File |\n|------|\n";
    "| `\(.path)` |") +
  section("Unused exports"; "unused_exports";
    "Exported symbols with no known consumers.\n\n| File | Line | Export |\n|------|-----:|--------|\n";
    "| `\(.path)` | \(.line) | `\(.export_name)`\(if .is_re_export then " *(re-export)*" else "" end) |") +
  section("Unused types"; "unused_types";
    "Type exports with no known consumers.\n\n| File | Line | Type |\n|------|-----:|------|\n";
    "| `\(.path)` | \(.line) | `\(.export_name)` |") +
  section("Unused dependencies"; "unused_dependencies";
    "Listed in `dependencies` but never imported.\n\n| Package |\n|---------|\n";
    "| `\(.package_name)` |") +
  section("Unused devDependencies"; "unused_dev_dependencies";
    "Listed in `devDependencies` but never referenced.\n\n| Package |\n|---------|\n";
    "| `\(.package_name)` |") +
  section("Unused optionalDependencies"; "unused_optional_dependencies";
    "Listed in `optionalDependencies` but never imported.\n\n| Package |\n|---------|\n";
    "| `\(.package_name)` |") +
  section("Unused enum members"; "unused_enum_members";
    "| File | Line | Enum | Member |\n|------|-----:|------|--------|\n";
    "| `\(.path)` | \(.line) | `\(.parent_name)` | `\(.member_name)` |") +
  section("Unused class members"; "unused_class_members";
    "| File | Line | Class | Member |\n|------|-----:|-------|--------|\n";
    "| `\(.path)` | \(.line) | `\(.parent_name)` | `\(.member_name)` |") +
  section("Unresolved imports"; "unresolved_imports";
    "Imports that could not be resolved. Check for missing packages or broken paths.\n\n| File | Line | Import |\n|------|-----:|--------|\n";
    "| `\(.path)` | \(.line) | `\(.specifier)` |") +
  section("Unlisted dependencies"; "unlisted_dependencies";
    "Imported but not declared in `package.json`.\n\n| Package | Used in |\n|---------|--------|\n";
    "| `\(.package_name)` | \(if (.imported_from | length) > 0 then (.imported_from[:3] | map("`\(.path):\(.line)`") | join(", ")) + (if (.imported_from | length) > 3 then " *+\((.imported_from | length) - 3) more*" else "" end) else "" end) |") +
  section("Duplicate exports"; "duplicate_exports";
    "Same name exported from multiple modules.\n\n| Export | Locations |\n|--------|-----------|\n";
    "| `\(.export_name)` | \(.locations[:3] | map("`\(.path):\(.line)`") | join(", "))\(if (.locations | length) > 3 then " *+\((.locations | length) - 3) more*" else "" end) |") +
  section("Circular dependencies"; "circular_dependencies";
    "Import cycles degrade tree-shaking and can cause runtime issues.\n\n| Cycle | Length |\n|-------|-------:|\n";
    "| \(.files | join(" \u2192 ")) | \(.length) |") +
  section("Type-only dependencies"; "type_only_dependencies";
    "Production deps only used via `import type` \u2014 consider moving to `devDependencies`.\n\n| Package |\n|---------|\n";
    "| `\(.package_name)` |") +
  "\n\n> :bulb: Run `fallow fix --dry-run` to preview safe auto-fixes for unused exports, enum members, and dependencies.\n> Add `// fallow-ignore-next-line` above a line to suppress a specific finding."
end
