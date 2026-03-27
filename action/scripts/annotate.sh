#!/usr/bin/env bash
set -eo pipefail

# Emit inline PR annotations via workflow commands
# Required env: FALLOW_COMMAND, MAX_ANNOTATIONS, ACTION_JQ_DIR

MAX="${MAX_ANNOTATIONS:-50}"
if ! [[ "$MAX" =~ ^[0-9]+$ ]]; then
  echo "::warning::max-annotations must be a positive integer, got: ${MAX_ANNOTATIONS}. Using default: 50"
  MAX=50
fi

# Detect package manager from lock files
PKG_MANAGER="npm"
ROOT="${FALLOW_ROOT:-.}"
if [ -f "${ROOT}/pnpm-lock.yaml" ] || [ -f "pnpm-lock.yaml" ]; then
  PKG_MANAGER="pnpm"
elif [ -f "${ROOT}/yarn.lock" ] || [ -f "yarn.lock" ]; then
  PKG_MANAGER="yarn"
fi
export PKG_MANAGER

ANNOTATIONS_FILE=$(mktemp)
: > "$ANNOTATIONS_FILE"

case "$FALLOW_COMMAND" in
  dead-code|check)
    jq -r -f "${ACTION_JQ_DIR}/annotations-check.jq" fallow-results.json > "$ANNOTATIONS_FILE" 2>/dev/null || true ;;
  dupes)
    jq -r -f "${ACTION_JQ_DIR}/annotations-dupes.jq" fallow-results.json > "$ANNOTATIONS_FILE" 2>/dev/null || true ;;
  health)
    jq -r -f "${ACTION_JQ_DIR}/annotations-health.jq" fallow-results.json > "$ANNOTATIONS_FILE" 2>/dev/null || true ;;
  fix) ;;
  "")
    {
      jq '.check // empty' fallow-results.json | jq -r -f "${ACTION_JQ_DIR}/annotations-check.jq" 2>/dev/null || true
      jq '.health // empty' fallow-results.json | jq -r -f "${ACTION_JQ_DIR}/annotations-health.jq" 2>/dev/null || true
      jq '.dupes // empty' fallow-results.json | jq -r -f "${ACTION_JQ_DIR}/annotations-dupes.jq" 2>/dev/null || true
    } > "$ANNOTATIONS_FILE" ;;
esac

TOTAL=$(wc -l < "$ANNOTATIONS_FILE" | tr -d ' ')
if [ "$TOTAL" -gt 0 ]; then
  head -n "$MAX" "$ANNOTATIONS_FILE"
  if [ "$TOTAL" -gt "$MAX" ]; then
    echo "::notice::Showing ${MAX} of ${TOTAL} annotations. Increase max-annotations to see more."
  fi
fi

rm -f "$ANNOTATIONS_FILE"
