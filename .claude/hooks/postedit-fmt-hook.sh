#!/usr/bin/env bash
# PostToolUse hook: run formatters on modified files
f=$(jq -r '.tool_input.file_path')
if [[ "$f" == *.rs ]]; then
    cargo fmt -q -- "$f" || true
elif [[ "$f" == *.ts || "$f" == *.tsx ]]; then
    cd "$CLAUDE_PROJECT_DIR/src/frontend" || exit 0
    npx prettier --write "$f" 2>/dev/null || true
    cd - || exit 0
fi
