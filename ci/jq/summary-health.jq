# GitLab variant of summary-health.jq
# Differences from GitHub: no > [!NOTE] / > [!WARNING] callouts

if (.findings | length) == 0 then
  "## Fallow \u2014 Code Complexity\n\n" +
  "> **No functions exceed complexity thresholds** \u00b7 \(.elapsed_ms)ms\n\n" +
  "\(.summary.functions_analyzed) functions analyzed (max cyclomatic: \(.summary.max_cyclomatic_threshold), max cognitive: \(.summary.max_cognitive_threshold))"
else
  "## Fallow \u2014 Code Complexity\n\n" +
  "> :warning: **\(.summary.functions_above_threshold) function\(if .summary.functions_above_threshold == 1 then "" else "s" end) exceed\(if .summary.functions_above_threshold == 1 then "s" else "" end) thresholds** \u00b7 \(.elapsed_ms)ms\n\n" +
  "| File | Function | Cyclomatic | Cognitive | Lines |\n|:-----|:---------|:-----------|:----------|:------|\n" +
  ([.findings[:25][] |
    "| `\(.path):\(.line)` | `\(.name)` | \(.cyclomatic)\(if .exceeded == "cyclomatic" or .exceeded == "both" then " **!**" else "" end) | \(.cognitive)\(if .exceeded == "cognitive" or .exceeded == "both" then " **!**" else "" end) | \(.line_count) |"
  ] | join("\n")) +
  (if (.findings | length) > 25 then "\n\n> \((.findings | length) - 25) more \u2014 run `fallow health` locally for the full list" else "" end) +
  "\n\n**\(.summary.files_analyzed)** files, **\(.summary.functions_analyzed)** functions analyzed (thresholds: cyclomatic > \(.summary.max_cyclomatic_threshold), cognitive > \(.summary.max_cognitive_threshold))"
end
