#!/usr/bin/env bash
set -eo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "⚙️  Configuring Git hooks for zserde..."
cd "$REPO_ROOT"

chmod +x .githooks/*
git config core.hooksPath .githooks

echo "✅ Git hooks configured! (.githooks/ -> core.hooksPath)"
