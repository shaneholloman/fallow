#!/usr/bin/env bash
# Test suite for fallow GitLab CI jq scripts and bash helpers
# Run: bash ci/tests/run.sh

set -o pipefail

DIR="$(cd "$(dirname "$0")" && pwd)"
CI_JQ_DIR="$DIR/../jq"
SHARED_JQ_DIR="$DIR/../../action/jq"
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

# =========================================================================
# GitLab-specific summary jq tests
# =========================================================================

echo ""
echo "=== GitLab Summary scripts ==="

echo "  summary-check.jq (GitLab):"
OUT=$(jq -r -f "$CI_JQ_DIR/summary-check.jq" "$FIXTURES/check.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow Dead Code Analysis" "has title"
assert_contains "$OUT" "issues" "mentions issues"
assert_contains "$OUT" "Unused" "lists unused categories"
assert_not_contains "$OUT" '!\[NOTE\]' "no GitHub callout NOTE"
assert_not_contains "$OUT" '!\[WARNING\]' "no GitHub callout WARNING"
assert_not_contains "$OUT" '!\[TIP\]' "no GitHub callout TIP"

OUT_CLEAN=$(jq -r -f "$CI_JQ_DIR/summary-check.jq" "$FIXTURES/check-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No issues found" "clean: shows no issues"

echo "  summary-health.jq (GitLab):"
OUT=$(jq -r -f "$CI_JQ_DIR/summary-health.jq" "$FIXTURES/health.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_not_contains "$OUT" '!\[NOTE\]' "no GitHub callout NOTE"
assert_not_contains "$OUT" '!\[WARNING\]' "no GitHub callout WARNING"

OUT_CLEAN=$(jq -r -f "$CI_JQ_DIR/summary-health.jq" "$FIXTURES/health-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No functions exceed" "clean: no functions exceed"

echo "  summary-combined.jq (GitLab):"
OUT=$(jq -r -f "$CI_JQ_DIR/summary-combined.jq" "$FIXTURES/combined.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow" "has title"
assert_contains "$OUT" "dead code" "mentions dead code"
assert_contains "$OUT" "Maintainability" "shows vital signs"
assert_not_contains "$OUT" '!\[NOTE\]' "no GitHub callout NOTE"
assert_not_contains "$OUT" '!\[TIP\]' "no GitHub callout TIP"

OUT_CLEAN=$(jq -r -f "$CI_JQ_DIR/summary-combined.jq" "$FIXTURES/combined-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No issues found" "clean: no issues"

# =========================================================================
# Shared summary scripts (reused from action/jq/, should still work)
# =========================================================================

echo ""
echo "=== Shared Summary scripts (from action/jq/) ==="

echo "  summary-dupes.jq:"
OUT=$(jq -r -f "$SHARED_JQ_DIR/summary-dupes.jq" "$FIXTURES/dupes.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "clone groups" "mentions clone groups"
assert_contains "$OUT" "Duplicated lines" "shows duplication stats"

OUT_CLEAN=$(jq -r -f "$SHARED_JQ_DIR/summary-dupes.jq" "$FIXTURES/dupes-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No code duplication" "clean: no duplication"

echo "  summary-fix.jq:"
# summary-fix needs fix results — test with combined (may not have fix data)
# Just verify it doesn't crash on missing data
OUT=$(echo '{"fixes":[],"dry_run":true}' | jq -r -f "$SHARED_JQ_DIR/summary-fix.jq" 2>&1)
assert_contains "$OUT" "No fixable issues" "empty fix: no fixable issues"

# =========================================================================
# GitLab review comments (dupes variant with GitLab URLs)
# =========================================================================

echo ""
echo "=== GitLab Review comment scripts ==="

export PREFIX="website/" MAX=50 FALLOW_ROOT="website" CI_PROJECT_URL="https://gitlab.com/test/repo" CI_COMMIT_SHA="abc123"

echo "  review-comments-dupes.jq (GitLab):"
OUT=$(jq -f "$CI_JQ_DIR/review-comments-dupes.jq" "$FIXTURES/dupes.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"
assert_contains "$OUT" "duplication" "mentions duplication"
assert_contains "$OUT" "gitlab.com" "has GitLab links (not GitHub)"
assert_not_contains "$OUT" "github.com" "no GitHub links"
assert_contains "$OUT" "View duplicated code" "includes code fragment"

# =========================================================================
# Shared review comment scripts (from action/jq/)
# =========================================================================

echo ""
echo "=== Shared Review comment scripts (from action/jq/) ==="

# Re-export env vars for shared jq scripts (they use GH_REPO etc. but we test with GitLab env)
export GH_REPO="" PR_NUMBER="" PR_HEAD_SHA=""

echo "  review-comments-check.jq:"
OUT=$(jq -f "$SHARED_JQ_DIR/review-comments-check.jq" "$FIXTURES/check.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"
assert_contains "$OUT" "Unused" "contains unused findings"
assert_contains "$OUT" "@public" "mentions @public JSDoc tag"
assert_contains "$OUT" "docs.fallow.tools" "has docs links"
assert_contains "$OUT" "Configure or suppress" "has suppress link"

OUT_CLEAN=$(jq -f "$SHARED_JQ_DIR/review-comments-check.jq" "$FIXTURES/check-clean.json" 2>&1)
assert_json_length "$OUT_CLEAN" "0" "clean: no comments"

echo "  review-comments-health.jq:"
OUT=$(jq -f "$SHARED_JQ_DIR/review-comments-health.jq" "$FIXTURES/health.json" 2>&1)
assert_valid_json "$OUT" "produces valid JSON"

echo "  review-body.jq:"
OUT=$(jq -r -f "$SHARED_JQ_DIR/review-body.jq" "$FIXTURES/combined.json" 2>&1)
assert_valid_markdown "$OUT" "produces output"
assert_contains "$OUT" "Fallow Review" "has review title"
assert_contains "$OUT" "fallow-review" "has marker comment"
assert_contains "$OUT" "Maintainability" "shows metrics"

# =========================================================================
# Suggestion block tests
# =========================================================================

echo ""
echo "=== Suggestion blocks ==="

echo "  unused-export type field:"
OUT=$(jq -f "$SHARED_JQ_DIR/review-comments-check.jq" "$FIXTURES/check.json" 2>&1)
TYPES=$(echo "$OUT" | jq -r '[.[].type] | unique | join(",")')
assert_contains "$TYPES" "unused-export" "exports have type field for suggestion enrichment"

echo "  single export keeps type:"
SINGLE='{"total_issues":1,"unused_files":[],"unused_exports":[{"path":"x.ts","export_name":"foo","is_type_only":false,"line":5,"col":0,"span_start":0,"is_re_export":false}],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"type_only_dependencies":[]}'
OUT=$(echo "$SINGLE" | jq -f "$SHARED_JQ_DIR/review-comments-check.jq" 2>&1)
assert_json_length "$OUT" "1" "single export produces 1 comment"
SINGLE_TYPE=$(echo "$OUT" | jq -r '.[0].type')
[ "$SINGLE_TYPE" = "unused-export" ] && pass "type is unused-export (not grouped)" || fail "type is unused-export" "got $SINGLE_TYPE"

echo "  grouped exports get different type:"
MULTI='{"total_issues":2,"unused_files":[],"unused_exports":[{"path":"x.ts","export_name":"foo","is_type_only":false,"line":5,"col":0,"span_start":0,"is_re_export":false},{"path":"x.ts","export_name":"bar","is_type_only":false,"line":10,"col":0,"span_start":0,"is_re_export":false}],"unused_types":[],"unused_dependencies":[],"unused_dev_dependencies":[],"unused_optional_dependencies":[],"unused_enum_members":[],"unused_class_members":[],"unresolved_imports":[],"unlisted_dependencies":[],"duplicate_exports":[],"circular_dependencies":[],"type_only_dependencies":[]}'
OUT=$(echo "$MULTI" | jq -f "$SHARED_JQ_DIR/review-comments-check.jq" | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "2 exports from same file grouped into 1"
GROUP_TYPE=$(echo "$OUT" | jq -r '.[0].type')
[ "$GROUP_TYPE" = "unused-export-group" ] && pass "grouped type is unused-export-group" || fail "grouped type" "got $GROUP_TYPE"
assert_contains "$OUT" "2 unused exports" "grouped comment mentions count"

echo "  review-body clean state:"
OUT_CLEAN=$(jq -r -f "$SHARED_JQ_DIR/review-body.jq" "$FIXTURES/combined-clean.json" 2>&1)
assert_contains "$OUT_CLEAN" "No dead code" "clean: no dead code"
assert_contains "$OUT_CLEAN" "No duplication" "clean: no duplication"
assert_contains "$OUT_CLEAN" "fallow-review" "clean: has marker"

# =========================================================================
# Merge script tests (shared from action/jq/)
# =========================================================================

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
OUT=$(echo "$EXPORTS" | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
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
OUT=$(echo "$CLONES" | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_valid_json "$OUT" "valid JSON"
assert_json_length "$OUT" "2" "deduplicates to 1 per clone group (4 → 2)"

# Test drop refactoring targets
TARGETS='[
  {"type":"other","path":"a.ts","line":1,"body":"finding"},
  {"type":"refactoring-target","path":"a.ts","line":1,"body":"target"}
]'
OUT=$(echo "$TARGETS" | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "drops refactoring targets"
assert_not_contains "$OUT" "target" "target body is removed"

# Test merge same line
SAME_LINE='[
  {"type":"other","path":"a.ts","line":5,"body":"complexity warning"},
  {"type":"other","path":"a.ts","line":5,"body":"unused export warning"}
]'
OUT=$(echo "$SAME_LINE" | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "1" "merges same-line comments"
assert_contains "$OUT" "complexity warning" "merged comment has first body"
assert_contains "$OUT" "unused export warning" "merged comment has second body"
assert_contains "$OUT" "\\\\n---\\\\n" "merged comment has separator"

# Test empty input
OUT=$(echo '[]' | jq --argjson max 50 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "0" "empty input produces empty output"

# Test max limit
MANY='[
  {"type":"other","path":"a.ts","line":1,"body":"1"},
  {"type":"other","path":"a.ts","line":2,"body":"2"},
  {"type":"other","path":"a.ts","line":3,"body":"3"},
  {"type":"other","path":"a.ts","line":4,"body":"4"},
  {"type":"other","path":"a.ts","line":5,"body":"5"}
]'
OUT=$(echo "$MANY" | jq --argjson max 3 -f "$SHARED_JQ_DIR/merge-comments.jq" 2>&1)
assert_json_length "$OUT" "3" "respects max limit"

# =========================================================================
# GitLab-specific: no GitHub callouts in any output
# =========================================================================

echo ""
echo "=== GitLab markdown compatibility ==="

echo "  verify no GitHub-specific callouts in GitLab scripts:"
for jq_file in "$CI_JQ_DIR"/*.jq; do
  name=$(basename "$jq_file")
  if /usr/bin/grep -q '!\[NOTE\]\|!\[WARNING\]\|!\[TIP\]\|!\[IMPORTANT\]\|!\[CAUTION\]' "$jq_file" 2>/dev/null; then
    fail "$name" "contains GitHub callout syntax"
  else
    pass "$name has no GitHub callouts"
  fi
done

echo "  verify GitLab dupes links use CI_PROJECT_URL:"
if /usr/bin/grep -q 'CI_PROJECT_URL' "$CI_JQ_DIR/review-comments-dupes.jq" 2>/dev/null; then
  pass "review-comments-dupes.jq uses CI_PROJECT_URL"
else
  fail "review-comments-dupes.jq" "missing CI_PROJECT_URL reference"
fi

if /usr/bin/grep -q 'GH_REPO' "$CI_JQ_DIR/review-comments-dupes.jq" 2>/dev/null; then
  fail "review-comments-dupes.jq" "still references GH_REPO"
else
  pass "review-comments-dupes.jq has no GH_REPO reference"
fi

# =========================================================================
# GitLab CI YAML structure tests
# =========================================================================

echo ""
echo "=== GitLab CI YAML structure ==="

CI_YAML="$DIR/../gitlab-ci.yml"

echo "  gitlab-ci.yml:"
assert_contains "$(cat "$CI_YAML")" "FALLOW_REVIEW" "has FALLOW_REVIEW variable"
assert_contains "$(cat "$CI_YAML")" "FALLOW_MAX_COMMENTS" "has FALLOW_MAX_COMMENTS variable"
assert_contains "$(cat "$CI_YAML")" "FALLOW_COMMENT" "has FALLOW_COMMENT variable"
assert_contains "$(cat "$CI_YAML")" "FALLOW_CODEQUALITY" "has FALLOW_CODEQUALITY variable"
assert_contains "$(cat "$CI_YAML")" "CI_MERGE_REQUEST_DIFF_BASE_SHA" "auto changed-since uses diff base SHA"
assert_contains "$(cat "$CI_YAML")" "comment.sh" "references comment.sh"
assert_contains "$(cat "$CI_YAML")" "review.sh" "references review.sh"
assert_contains "$(cat "$CI_YAML")" "gl-code-quality-report" "generates Code Quality report"
assert_contains "$(cat "$CI_YAML")" "suggestion" "mentions suggestion blocks in docs"

# =========================================================================
# Bash script structure tests
# =========================================================================

echo ""
echo "=== Bash script structure ==="

SCRIPTS_DIR="$DIR/../scripts"

echo "  comment.sh:"
assert_contains "$(cat "$SCRIPTS_DIR/comment.sh")" "PRIVATE-TOKEN" "supports GITLAB_TOKEN"
assert_contains "$(cat "$SCRIPTS_DIR/comment.sh")" "JOB-TOKEN" "supports CI_JOB_TOKEN"
assert_contains "$(cat "$SCRIPTS_DIR/comment.sh")" "fallow-results" "uses fallow-results marker"
assert_contains "$(cat "$SCRIPTS_DIR/comment.sh")" "PUT" "can update existing comment"
assert_contains "$(cat "$SCRIPTS_DIR/comment.sh")" "POST" "can create new comment"

echo "  review.sh:"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "discussions" "uses GitLab Discussions API"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "position" "posts with position for inline comments"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "suggestion" "adds suggestion blocks"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "merge-comments" "runs merge pipeline"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "fallow-review" "uses fallow-review marker"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "DELETE" "cleans up previous comments"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "unused-export" "handles unused export suggestions"
assert_contains "$(cat "$SCRIPTS_DIR/review.sh")" "FALLOW_SHARED_JQ_DIR" "can use shared jq scripts"

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
