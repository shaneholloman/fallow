#!/usr/bin/env bash
# Test suite for fallow GitHub Action jq scripts and bash helpers
# Run: bash action/tests/run.sh

set -o pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
JQ_DIR="$DIR/../jq"
FIXTURES="$DIR/fixtures"
PASSED=0
FAILED=0
ERRORS=()

# --- Helpers ---

pass() { PASSED=$((PASSED + 1)); echo "  ✓ $1"; }
fail() { FAILED=$((FAILED + 1)); ERRORS+=("$1: $2"); echo "  ✗ $1 — $2"; }

assert_contains() {
  local output="$1" expected="$2" name="$3"
  if echo "$output" | /usr/bin/grep -q "$expected" 2>/dev/null; then
    pass "$name"
  else
    fail "$name" "expected to contain: $expected"
  fi
}

assert_not_contains() {
  local output="$1" unexpected="$2" name="$3"
  if echo "$output" | /usr/bin/grep -q "$unexpected" 2>/dev/null; then
    fail "$name" "should NOT contain: $unexpected"
  else
    pass "$name"
  fi
}

assert_json_length() {
  local output="$1" expected="$2" name="$3"
  local actual
  actual=$(echo "$output" | jq 'length' 2>/dev/null)
  if [ "$actual" = "$expected" ]; then
    pass "$name"
  else
    fail "$name" "expected length $expected, got $actual"
  fi
}

assert_valid_json() {
  local output="$1" name="$2"
  if echo "$output" | jq -e '.' > /dev/null 2>&1; then
    pass "$name"
  else
    fail "$name" "invalid JSON output"
  fi
}

assert_valid_markdown() {
  local output="$1" name="$2"
  if [ -n "$output" ]; then
    pass "$name"
  else
    fail "$name" "empty markdown output"
  fi
}

assert_json_value() {
  local output="$1" jq_expr="$2" expected="$3" name="$4"
  local actual
  actual=$(echo "$output" | jq -r "$jq_expr" 2>/dev/null)
  if [ "$actual" = "$expected" ]; then
    pass "$name"
  else
    fail "$name" "expected $expected, got $actual"
  fi
}

# --- Summary jq tests ---

echo ""
echo "=== Summary scripts ==="

echo "  summary-check.jq:"
OUT=$(jq -r -f "$JQ_DIR/summary-check.jq" "$FIXTURES/check.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow Dead Code Analysis" "has title"
assert_contains "$OUT" "issues" "mentions issues"
assert_contains "$OUT" "Unused" "lists unused categories"

OUT_CLEAN=$(jq -r -f "$JQ_DIR/summary-check.jq" "$FIXTURES/check-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No issues found" "clean: shows no issues"
assert_not_contains "$OUT_CLEAN" "WARNING" "clean: no warning"

echo "  summary-dupes.jq:"
OUT=$(jq -r -f "$JQ_DIR/summary-dupes.jq" "$FIXTURES/dupes.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "clone groups" "mentions clone groups"
assert_contains "$OUT" "Duplicated lines" "shows duplication stats"

OUT_CLEAN=$(jq -r -f "$JQ_DIR/summary-dupes.jq" "$FIXTURES/dupes-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No code duplication" "clean: no duplication"

echo "  summary-health.jq:"
OUT=$(jq -r -f "$JQ_DIR/summary-health.jq" "$FIXTURES/health.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"

OUT_CLEAN=$(jq -r -f "$JQ_DIR/summary-health.jq" "$FIXTURES/health-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No functions exceed" "clean: no functions exceed"

echo "  summary-combined.jq:"
OUT=$(jq -r -f "$JQ_DIR/summary-combined.jq" "$FIXTURES/combined.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow" "has title"
assert_contains "$OUT" "dead code" "mentions dead code"
assert_contains "$OUT" "Maintainability" "shows vital signs"

OUT_CLEAN=$(jq -r -f "$JQ_DIR/summary-combined.jq" "$FIXTURES/combined-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No issues found" "clean: no issues"

# --- Annotation jq tests ---

echo ""
echo "=== Annotation scripts ==="

echo "  annotations-check.jq:"
OUT=$(jq -r -f "$JQ_DIR/annotations-check.jq" "$FIXTURES/check.json" 2>&1)
assert_contains "$OUT" "::warning" "emits warning commands"
assert_contains "$OUT" "file=" "has file references"
assert_contains "$OUT" "title=" "has titles"

OUT_CLEAN=$(jq -r -f "$JQ_DIR/annotations-check.jq" "$FIXTURES/check-clean.json" 2>&1)
[ -z "$OUT_CLEAN" ] && pass "clean: no annotations" || fail "clean: no annotations" "got output"

echo "  annotations-dupes.jq:"
OUT=$(jq -r -f "$JQ_DIR/annotations-dupes.jq" "$FIXTURES/dupes.json" 2>&1)
assert_contains "$OUT" "::warning" "emits warning commands"
assert_contains "$OUT" "Code duplication" "mentions duplication"

echo "  annotations-health.jq:"
OUT=$(jq -r -f "$JQ_DIR/annotations-health.jq" "$FIXTURES/health.json" 2>&1)
# health-clean has no findings, so no output expected even from non-clean
# (our fixture might have no findings above threshold)
assert_valid_markdown "$OUT" "produces output or empty"

# --- Review comment jq tests ---

echo ""
echo "=== Review comment scripts ==="

export PREFIX="website/" MAX=50 FALLOW_ROOT="website" GH_REPO="test/repo" PR_NUMBER=1 PR_HEAD_SHA="abc123"

echo "  review-comments-check.jq:"
OUT=$(jq -f "$JQ_DIR/review-comments-check.jq" "$FIXTURES/check.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"
assert_contains "$OUT" "Unused" "contains unused findings"
assert_contains "$OUT" "@public" "mentions @public JSDoc tag"
assert_contains "$OUT" "docs.fallow.tools" "has docs links"
assert_contains "$OUT" "Configure or suppress" "has suppress link"

OUT_CLEAN=$(jq -f "$JQ_DIR/review-comments-check.jq" "$FIXTURES/check-clean.json" 2>&1)
assert_json_length "$OUT_CLEAN" "0" "clean: no comments"

echo "  review-comments-dupes.jq:"
OUT=$(jq -f "$JQ_DIR/review-comments-dupes.jq" "$FIXTURES/dupes.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"
assert_contains "$OUT" "duplication" "mentions duplication"
assert_contains "$OUT" "github.com" "has GitHub links"
assert_contains "$OUT" "View duplicated code" "includes code fragment"

echo "  review-comments-health.jq:"
OUT=$(jq -f "$JQ_DIR/review-comments-health.jq" "$FIXTURES/health.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"

echo "  review-body.jq:"
OUT=$(jq -r -f "$JQ_DIR/review-body.jq" "$FIXTURES/combined.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow Review" "has review title"
assert_contains "$OUT" "fallow-review" "has marker comment"
assert_contains "$OUT" "Maintainability" "shows metrics"

# --- Suggestion block tests ---

echo ""
echo "=== Suggestion blocks ==="

echo "  unused-export type field:"
OUT=$(jq -f "$JQ_DIR/review-comments-check.jq" "$FIXTURES/check.json" 2>&1)
TYPES=$(echo "$OUT" | jq -r '[.[].type] | unique | join(",")')
assert_contains "$TYPES" "unused-export" "exports have type field for suggestion enrichment"

echo "  single export keeps type:"
SINGLE='{"total_issues":1,"unused_files":[],"unused_exports":[{"path":"x.ts","export_name":"foo","is_type_only":false,"line":5,"col":0,"span_start":0,"is_re_export":false}],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"boundary_violations":[],"type_only_dependencies":[]}'
OUT=$(echo "$SINGLE" | jq -f "$JQ_DIR/review-comments-check.jq" 2>&1)
assert_json_length "$OUT" "1" "single export produces 1 comment"
SINGLE_TYPE=$(echo "$OUT" | jq -r '.[0].type')
[ "$SINGLE_TYPE" = "unused-export" ] && pass "type is unused-export (not grouped)" || fail "type is unused-export" "got $SINGLE_TYPE"

echo "  grouped exports get different type:"
MULTI='{"total_issues":2,"unused_files":[],"unused_exports":[{"path":"x.ts","export_name":"foo","is_type_only":false,"line":5,"col":0,"span_start":0,"is_re_export":false},{"path":"x.ts","export_name":"bar","is_type_only":false,"line":10,"col":0,"span_start":0,"is_re_export":false}],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"boundary_violations":[],"type_only_dependencies":[]}'
OUT=$(echo "$MULTI" | jq -f "$JQ_DIR/review-comments-check.jq" | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "2 exports from same file grouped into 1"
GROUP_TYPE=$(echo "$OUT" | jq -r '.[0].type')
[ "$GROUP_TYPE" = "unused-export-group" ] && pass "grouped type is unused-export-group" || fail "grouped type" "got $GROUP_TYPE"
assert_contains "$OUT" "2 unused exports" "grouped comment mentions count"

echo "  boundary violation produces review comment:"
BV_INPUT='{"total_issues":1,"unused_files":[],"unused_exports":[],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"boundary_violations":[{"from_path":"src/ui/App.ts","to_path":"src/db/query.ts","from_zone":"ui","to_zone":"db","import_specifier":"src/db/query.ts","line":5,"col":9}],"type_only_dependencies":[]}'
OUT=$(echo "$BV_INPUT" | MAX=50 jq -f "$JQ_DIR/review-comments-check.jq" 2>&1)
assert_valid_json "$OUT" "boundary violation JSON valid"
assert_json_length "$OUT" "1" "boundary violation produces 1 comment"
assert_contains "$OUT" "Boundary violation" "comment mentions boundary violation"
assert_contains "$OUT" "ui" "comment mentions from_zone"
assert_contains "$OUT" "db" "comment mentions to_zone"
assert_contains "$OUT" "src/ui/App.ts" "comment mentions from_path"
assert_contains "$OUT" "src/db/query.ts" "comment mentions to_path"
BV_PATH=$(echo "$OUT" | jq -r '.[0].path')
[ "$BV_PATH" = "${PREFIX}src/ui/App.ts" ] && pass "path has prefix + from_path" || fail "path has prefix + from_path" "got $BV_PATH"
BV_LINE=$(echo "$OUT" | jq -r '.[0].line')
[ "$BV_LINE" = "5" ] && pass "line is 5" || fail "line is 5" "got $BV_LINE"

echo "  boundary violation produces annotation:"
ANN=$(echo "$BV_INPUT" | jq -rf "$JQ_DIR/annotations-check.jq" 2>&1)
assert_contains "$ANN" "::warning" "annotation is warning level"
assert_contains "$ANN" "file=src/ui/App.ts" "annotation has correct file"
assert_contains "$ANN" "line=5" "annotation has correct line"
assert_contains "$ANN" "Boundary violation" "annotation title"
assert_contains "$ANN" "zone" "annotation mentions zone"

echo "  boundary violation appears in summary:"
SUMMARY=$(echo "$BV_INPUT" | jq -rf "$JQ_DIR/summary-check.jq" 2>&1)
assert_contains "$SUMMARY" "Boundary violations" "summary has boundary section"
assert_contains "$SUMMARY" "src/ui/App.ts" "summary mentions file"
assert_contains "$SUMMARY" "ui" "summary mentions zone"

echo "  review-body clean state:"
OUT_CLEAN=$(jq -r -f "$JQ_DIR/review-body.jq" "$FIXTURES/combined-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No dead code" "clean: no dead code"
assert_contains "$OUT_CLEAN" "No duplication" "clean: no duplication"
assert_contains "$OUT_CLEAN" "fallow-review" "clean: has marker"

# --- Merge script tests ---

echo ""
echo "=== Merge script ==="

echo "  merge-comments.jq:"

# Test grouping unused exports
EXPORTS='[
  {"type":"unused-export","export_name":"foo","path":"a.ts","line":1,"body":"unused foo"},
  {"type":"unused-export","export_name":"bar","path":"a.ts","line":5,"body":"unused bar"},
  {"type":"unused-export","export_name":"baz","path":"b.ts","line":1,"body":"unused baz"},
  {"type":"other","path":"c.ts","line":1,"body":"something else"}
]'
OUT=$(echo "$EXPORTS" | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_length "$OUT" "3" "groups 2 exports from a.ts into 1 (2 + 1 other = 3)"
assert_contains "$OUT" "2 unused exports" "grouped comment mentions count"
assert_contains "$OUT" "foo" "grouped comment lists export names"
assert_contains "$OUT" "bar" "grouped comment lists export names"

# Test dedup clones
CLONES='[
  {"type":"duplication","group_id":"g1","path":"a.ts","line":5,"body":"clone 1 instance 1"},
  {"type":"duplication","group_id":"g1","path":"a.ts","line":20,"body":"clone 1 instance 2"},
  {"type":"duplication","group_id":"g2","path":"b.ts","line":10,"body":"clone 2 instance 1"},
  {"type":"duplication","group_id":"g2","path":"b.ts","line":30,"body":"clone 2 instance 2"}
]'
OUT=$(echo "$CLONES" | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_length "$OUT" "2" "deduplicates to 1 per clone group (4 → 2)"

# Test drop refactoring targets
TARGETS='[
  {"type":"other","path":"a.ts","line":1,"body":"finding"},
  {"type":"refactoring-target","path":"a.ts","line":1,"body":"target"}
]'
OUT=$(echo "$TARGETS" | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "drops refactoring targets"
assert_not_contains "$OUT" "target" "target body is removed"

# Test merge same line
SAME_LINE='[
  {"type":"other","path":"a.ts","line":5,"body":"complexity warning"},
  {"type":"other","path":"a.ts","line":5,"body":"unused export warning"}
]'
OUT=$(echo "$SAME_LINE" | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "merges same-line comments"
assert_contains "$OUT" "complexity warning" "merged comment has first body"
assert_contains "$OUT" "unused export warning" "merged comment has second body"
assert_contains "$OUT" "\\\\n---\\\\n" "merged comment has separator"

# Test empty input
OUT=$(echo '[]' | jq --argjson max 50 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "0" "empty input produces empty output"

# Test max limit
MANY='[
  {"type":"other","path":"a.ts","line":1,"body":"1"},
  {"type":"other","path":"a.ts","line":2,"body":"2"},
  {"type":"other","path":"a.ts","line":3,"body":"3"},
  {"type":"other","path":"a.ts","line":4,"body":"4"},
  {"type":"other","path":"a.ts","line":5,"body":"5"}
]'
OUT=$(echo "$MANY" | jq --argjson max 3 -f "$JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "3" "respects max limit"

# --- Changed-file filter tests ---

echo ""
echo "=== Changed-file filter (filter-changed.jq) ==="

echo "  check format:"
OUT=$(jq --argjson changed '["src/helpers/api.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/check.json" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_value "$OUT" '.unused_exports | length' "3" "keeps only exports in changed files"
assert_json_value "$OUT" '.unused_files | length' "0" "no unused files match changed path"
assert_json_value "$OUT" '.unused_dependencies | length' "3" "preserves dependency issues (not file-scoped)"
assert_json_value "$OUT" '.total_issues' "6" "recalculates total_issues"

echo "  check with no matching files:"
OUT=$(jq --argjson changed '["nonexistent.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/check.json" 2>&1)
assert_json_value "$OUT" '.unused_exports | length' "0" "filters all exports"
assert_json_value "$OUT" '.unused_dependencies | length' "3" "deps preserved even with no file matches"

echo "  check clean passthrough:"
OUT=$(jq --argjson changed '["src/a.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/check-clean.json" 2>&1)
assert_json_value "$OUT" '.total_issues' "0" "clean results stay at 0"

echo "  health format:"
OUT=$(jq --argjson changed '["src/helpers/content-parser.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/health.json" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_value "$OUT" '.file_scores | length' "1" "keeps only changed file scores"
assert_json_value "$OUT" '.file_scores[0].path' "src/helpers/content-parser.ts" "correct file retained"

echo "  dupes format:"
DUPES_PATH=$(jq -r '.clone_groups[0].instances[0].file' "$FIXTURES/dupes.json")
OUT=$(jq --argjson changed "[\"$DUPES_PATH\"]" -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/dupes.json" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_value "$OUT" '.stats.clone_groups' "1" "retains group with changed instance"

OUT=$(jq --argjson changed '["nonexistent.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/dupes.json" 2>&1)
assert_json_value "$OUT" '.stats.clone_groups' "0" "removes all groups when no match"

echo "  combined format:"
OUT=$(jq --argjson changed '["src/helpers/api.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/combined.json" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_value "$OUT" '.check.unused_exports | length' "3" "filters check sub-object"
assert_json_value "$OUT" '.check.total_issues' "6" "recalculates check total"

echo "  combined clean passthrough:"
OUT=$(jq --argjson changed '["src/a.ts"]' -f "$JQ_DIR/filter-changed.jq" "$FIXTURES/combined-clean.json" 2>&1)
assert_json_value "$OUT" '.check.total_issues' "0" "clean combined stays at 0"

echo "  boundary violation filter:"
BV_INPUT='{"total_issues":2,"unused_files":[],"unused_exports":[],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"boundary_violations":[{"from_path":"src/ui/App.ts","to_path":"src/db/query.ts","from_zone":"ui","to_zone":"db","import_specifier":"src/db/query.ts","line":5,"col":9},{"from_path":"src/api/handler.ts","to_path":"src/db/repo.ts","from_zone":"api","to_zone":"db","import_specifier":"src/db/repo.ts","line":10,"col":9}],"type_only_dependencies":[]}'
OUT=$(echo "$BV_INPUT" | jq --argjson changed '["src/ui/App.ts"]' -f "$JQ_DIR/filter-changed.jq" 2>&1)
assert_json_value "$OUT" '.boundary_violations | length' "1" "keeps only violations from changed files"
assert_json_value "$OUT" '.total_issues' "1" "recalculates total after filtering"

echo "  circular dependency filter:"
CD_INPUT='{"total_issues":1,"unused_files":[],"unused_exports":[],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[{"files":["src/a.ts","src/b.ts"],"length":2,"line":1,"col":0}],"boundary_violations":[],"type_only_dependencies":[]}'
OUT=$(echo "$CD_INPUT" | jq --argjson changed '["src/a.ts"]' -f "$JQ_DIR/filter-changed.jq" 2>&1)
assert_json_value "$OUT" '.circular_dependencies | length' "1" "keeps cycle if any file changed"
OUT=$(echo "$CD_INPUT" | jq --argjson changed '["src/c.ts"]' -f "$JQ_DIR/filter-changed.jq" 2>&1)
assert_json_value "$OUT" '.circular_dependencies | length' "0" "removes cycle if no file changed"

# --- Pre-computed changed files (shallow clone fallback) tests ---

echo ""
echo "=== Pre-computed changed files (fallow-changed-files.json) ==="

WORK_DIR=$(mktemp -d)
SCRIPTS_DIR="$DIR/../scripts"

# Copy fixtures into work dir to simulate the action working directory
cp "$FIXTURES/check.json" "$WORK_DIR/fallow-results.json"

echo "  comment.sh filtering with pre-computed file:"

# Create a pre-computed changed files list (what analyze.sh produces)
echo '["src/helpers/api.ts"]' > "$WORK_DIR/fallow-changed-files.json"

# Run the filtering logic from comment.sh in the work dir
OUT=$(cd "$WORK_DIR" && \
  CHANGED_SINCE="abc123" \
  INPUT_ROOT="." \
  ACTION_JQ_DIR="$JQ_DIR" \
  FALLOW_COMMAND="dead-code" \
  bash -c '
    RESULTS_FILE="fallow-results.json"
    CHANGED_JSON=""
    if [ -f fallow-changed-files.json ]; then
      CHANGED_JSON=$(cat fallow-changed-files.json)
    fi
    if [ -n "$CHANGED_JSON" ] && [ "$CHANGED_JSON" != "[]" ]; then
      if jq --argjson changed "$CHANGED_JSON" -f "${ACTION_JQ_DIR}/filter-changed.jq" fallow-results.json > fallow-results-scoped.json 2>/dev/null; then
        RESULTS_FILE="fallow-results-scoped.json"
      fi
    fi
    jq -r ".total_issues" "$RESULTS_FILE"
  ' 2>&1)
[ "$OUT" = "6" ] && pass "filters to 6 issues (pre-computed)" || fail "pre-computed filter" "expected 6, got $OUT"

echo "  fallback to unfiltered when no pre-computed file:"
rm -f "$WORK_DIR/fallow-changed-files.json"

# Without fallow-changed-files.json AND without git, falls through to unfiltered
OUT=$(cd "$WORK_DIR" && \
  CHANGED_SINCE="abc123" \
  INPUT_ROOT="." \
  ACTION_JQ_DIR="$JQ_DIR" \
  bash -c '
    RESULTS_FILE="fallow-results.json"
    CHANGED_JSON=""
    if [ -f fallow-changed-files.json ]; then
      CHANGED_JSON=$(cat fallow-changed-files.json)
    else
      CHANGED_FILES=$(git diff --name-only --relative "abc123...HEAD" -- . 2>/dev/null || true)
      if [ -n "$CHANGED_FILES" ]; then
        CHANGED_JSON=$(echo "$CHANGED_FILES" | jq -R -s "split(\"\n\") | map(select(length > 0))")
      fi
    fi
    if [ -n "$CHANGED_JSON" ] && [ "$CHANGED_JSON" != "[]" ]; then
      jq --argjson changed "$CHANGED_JSON" -f "${ACTION_JQ_DIR}/filter-changed.jq" fallow-results.json > fallow-results-scoped.json 2>/dev/null && RESULTS_FILE="fallow-results-scoped.json"
    fi
    jq -r ".total_issues" "$RESULTS_FILE"
  ' 2>&1)
EXPECTED_TOTAL=$(jq -r '.total_issues' "$FIXTURES/check.json")
[ "$OUT" = "$EXPECTED_TOTAL" ] && pass "unfiltered when no pre-computed file" || fail "no pre-computed fallback" "expected $EXPECTED_TOTAL, got $OUT"

echo "  empty changed list produces no filtering:"
echo '[]' > "$WORK_DIR/fallow-changed-files.json"
OUT=$(cd "$WORK_DIR" && \
  CHANGED_SINCE="abc123" \
  ACTION_JQ_DIR="$JQ_DIR" \
  bash -c '
    RESULTS_FILE="fallow-results.json"
    CHANGED_JSON=""
    if [ -f fallow-changed-files.json ]; then
      CHANGED_JSON=$(cat fallow-changed-files.json)
    fi
    if [ -n "$CHANGED_JSON" ] && [ "$CHANGED_JSON" != "[]" ]; then
      jq --argjson changed "$CHANGED_JSON" -f "${ACTION_JQ_DIR}/filter-changed.jq" fallow-results.json > fallow-results-scoped.json 2>/dev/null && RESULTS_FILE="fallow-results-scoped.json"
    fi
    jq -r ".total_issues" "$RESULTS_FILE"
  ' 2>&1)
[ "$OUT" = "$EXPECTED_TOTAL" ] && pass "empty list skips filtering" || fail "empty list guard" "expected $EXPECTED_TOTAL, got $OUT"

echo "  combined format with pre-computed file:"
cp "$FIXTURES/combined.json" "$WORK_DIR/fallow-results.json"
echo '["src/helpers/api.ts"]' > "$WORK_DIR/fallow-changed-files.json"
OUT=$(cd "$WORK_DIR" && \
  CHANGED_SINCE="abc123" \
  ACTION_JQ_DIR="$JQ_DIR" \
  bash -c '
    RESULTS_FILE="fallow-results.json"
    CHANGED_JSON=""
    if [ -f fallow-changed-files.json ]; then
      CHANGED_JSON=$(cat fallow-changed-files.json)
    fi
    if [ -n "$CHANGED_JSON" ] && [ "$CHANGED_JSON" != "[]" ]; then
      jq --argjson changed "$CHANGED_JSON" -f "${ACTION_JQ_DIR}/filter-changed.jq" fallow-results.json > fallow-results-scoped.json 2>/dev/null && RESULTS_FILE="fallow-results-scoped.json"
    fi
    jq -r ".check.total_issues" "$RESULTS_FILE"
  ' 2>&1)
[ "$OUT" = "6" ] && pass "combined format filters check section" || fail "combined pre-computed" "expected 6, got $OUT"

echo "  no CHANGED_SINCE skips filtering entirely:"
cp "$FIXTURES/check.json" "$WORK_DIR/fallow-results.json"
echo '["src/helpers/api.ts"]' > "$WORK_DIR/fallow-changed-files.json"
OUT=$(cd "$WORK_DIR" && \
  ACTION_JQ_DIR="$JQ_DIR" \
  bash -c '
    RESULTS_FILE="fallow-results.json"
    if [ -n "${CHANGED_SINCE:-}" ]; then
      echo "ERROR: should not enter filter block"
    fi
    jq -r ".total_issues" "$RESULTS_FILE"
  ' 2>&1)
[ "$OUT" = "$EXPECTED_TOTAL" ] && pass "no CHANGED_SINCE skips filtering" || fail "no CHANGED_SINCE guard" "expected $EXPECTED_TOTAL, got $OUT"

rm -rf "$WORK_DIR"

# --- Summary ---

echo ""
echo "================================"
echo "  $PASSED passed, $FAILED failed"
echo "================================"

if [ "$FAILED" -gt 0 ]; then
  echo ""
  echo "Failures:"
  for err in "${ERRORS[@]}"; do
    echo "  ✗ $err"
  done
  exit 1
fi
