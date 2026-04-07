def pct(n): n | . * 10 | round / 10;
def signed(n): if n > 0 then "+\(pct(n))" elif n < 0 then "\(pct(n))" else "0.0" end;
def metric_delta(name):
  (.health_trend.metrics // []) | map(select(.name == name)) | first // null;
def suppression_docs: "https://docs.fallow.tools/configuration/suppression";

# Health delta header (only when --score is present)
(if .health_score then
  (metric_delta("score")) as $score_delta |
  (metric_delta("dead_export_pct")) as $dead_delta |
  (metric_delta("avg_cyclomatic")) as $cx_delta |
  "> **Health: \(.health_score.grade) (\(pct(.health_score.score)))**" +
  (if $score_delta then
    " \u00b7 \(signed($score_delta.delta)) pts vs previous (\(.health_trend.compared_to.grade) \(pct(.health_trend.compared_to.score)))" +
    (if $dead_delta and $dead_delta.delta != 0 then
      " \u00b7 dead exports \(pct($dead_delta.current))% (\(signed($dead_delta.delta))%)" +
      (if $dead_delta.delta > 0 then " [suppress?](\(suppression_docs))" else "" end)
    else "" end) +
    (if $cx_delta and $cx_delta.delta != 0 then
      " \u00b7 complexity \(pct($cx_delta.current)) (\(signed($cx_delta.delta)))"
    else "" end)
  else "" end) +
  "\n\n"
else "" end) +

if (.findings | length) == 0 then
  "## Fallow \u2014 Code Complexity\n\n" +
  "> [!NOTE]\n> **No functions exceed complexity thresholds** \u00b7 \(.elapsed_ms)ms\n\n" +
  "\(.summary.functions_analyzed) functions analyzed (max cyclomatic: \(.summary.max_cyclomatic_threshold), max cognitive: \(.summary.max_cognitive_threshold))"
else
  "## Fallow \u2014 Code Complexity\n\n" +
  "> [!WARNING]\n> **\(.summary.functions_above_threshold) function\(if .summary.functions_above_threshold == 1 then "" else "s" end) exceed\(if .summary.functions_above_threshold == 1 then "s" else "" end) thresholds** \u00b7 \(.elapsed_ms)ms\n\n" +
  "| File | Function | Cyclomatic | Cognitive | Lines |\n|:-----|:---------|:-----------|:----------|:------|\n" +
  ([.findings[:25][] |
    "| `\(.path):\(.line)` | `\(.name)` | \(.cyclomatic)\(if .exceeded == "cyclomatic" or .exceeded == "both" then " **!**" else "" end) | \(.cognitive)\(if .exceeded == "cognitive" or .exceeded == "both" then " **!**" else "" end) | \(.line_count) |"
  ] | join("\n")) +
  (if (.findings | length) > 25 then "\n\n> \((.findings | length) - 25) more \u2014 run `fallow health` locally for the full list" else "" end) +
  "\n\n**\(.summary.files_analyzed)** files, **\(.summary.functions_analyzed)** functions analyzed (thresholds: cyclomatic > \(.summary.max_cyclomatic_threshold), cognitive > \(.summary.max_cognitive_threshold))"
end
