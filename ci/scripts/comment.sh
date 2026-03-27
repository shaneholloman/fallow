#!/usr/bin/env bash
set -eo pipefail

# Post or update an MR comment with analysis results
# Required env: GITLAB_TOKEN or CI_JOB_TOKEN, CI_API_V4_URL, CI_PROJECT_ID,
#   CI_MERGE_REQUEST_IID, FALLOW_COMMAND, FALLOW_JQ_DIR

# Auth header
if [ -n "${GITLAB_TOKEN:-}" ]; then
  AUTH_HEADER="PRIVATE-TOKEN: ${GITLAB_TOKEN}"
else
  AUTH_HEADER="JOB-TOKEN: ${CI_JOB_TOKEN}"
fi

API_URL="${CI_API_V4_URL}/projects/${CI_PROJECT_ID}/merge_requests/${CI_MERGE_REQUEST_IID}/notes"

# Select jq script — prefer GitLab-specific variants, fall back to shared
pick_jq() {
  local name="$1"
  if [ -f "${FALLOW_JQ_DIR}/${name}" ]; then
    echo "${FALLOW_JQ_DIR}/${name}"
  elif [ -f "${FALLOW_SHARED_JQ_DIR:-}/${name}" ]; then
    echo "${FALLOW_SHARED_JQ_DIR}/${name}"
  else
    echo "${FALLOW_JQ_DIR}/${name}"
  fi
}

case "$FALLOW_COMMAND" in
  dead-code|check) JQ_FILE=$(pick_jq "summary-check.jq") ;;
  dupes)           JQ_FILE=$(pick_jq "summary-dupes.jq") ;;
  health)          JQ_FILE=$(pick_jq "summary-health.jq") ;;
  fix)             JQ_FILE=$(pick_jq "summary-fix.jq") ;;
  "")              JQ_FILE=$(pick_jq "summary-combined.jq") ;;
  *)               echo "ERROR: Unexpected command: ${FALLOW_COMMAND}"; exit 2 ;;
esac

# For combined mode, pass the full JSON; for specific commands, extract section
INPUT_FILE="fallow-results.json"
if [ -z "$FALLOW_COMMAND" ]; then
  INPUT_FILE="fallow-results.json"
elif [ "$FALLOW_COMMAND" = "dead-code" ] || [ "$FALLOW_COMMAND" = "check" ]; then
  # If running in combined mode but requesting check summary
  if jq -e '.check' fallow-results.json > /dev/null 2>&1; then
    jq '.check' fallow-results.json > /tmp/fallow-comment-input.json
    INPUT_FILE="/tmp/fallow-comment-input.json"
  fi
fi

# Generate comment body
if ! COMMENT_BODY=$(jq -r -f "$JQ_FILE" "$INPUT_FILE"); then
  echo "WARNING: Failed to generate MR comment body"
  exit 0
fi
COMMENT_BODY="${COMMENT_BODY}

<!-- fallow-results -->"

# Find existing fallow comment to update (avoids spam on busy MRs)
EXISTING_NOTE_ID=$(curl -sf \
  --header "${AUTH_HEADER}" \
  "${API_URL}?per_page=100" \
  | jq -r '.[] | select(.body | contains("<!-- fallow-results -->")) | .id' \
  | head -1) || true

if [ -n "$EXISTING_NOTE_ID" ]; then
  curl -sf \
    --header "${AUTH_HEADER}" \
    --header "Content-Type: application/json" \
    --request PUT \
    --data "$(jq -n --arg body "$COMMENT_BODY" '{body: $body}')" \
    "${API_URL}/${EXISTING_NOTE_ID}" > /dev/null \
    && echo "Updated existing MR comment" \
    || echo "WARNING: Failed to update MR comment (check token permissions)"
else
  curl -sf \
    --header "${AUTH_HEADER}" \
    --header "Content-Type: application/json" \
    --request POST \
    --data "$(jq -n --arg body "$COMMENT_BODY" '{body: $body}')" \
    "${API_URL}" > /dev/null \
    && echo "Created new MR comment" \
    || echo "WARNING: Failed to create MR comment (check token permissions)"
fi
