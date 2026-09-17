#!/bin/bash
# ============================================================
# SENN OSS Self-host Shutdown Script
# 
# Usage:
#   ./scripts/oss-down.sh          # Stop services (keep data)
#   ./scripts/oss-down.sh --volumes # Stop and delete all data
# ============================================================

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

ENV_OSS_FILE=".env.oss"

echo "🛑 SENN OSS Self-host Shutdown"
echo "================================"
echo ""

# Check for --volumes flag
VOLUMES_FLAG=""
if [ "$1" = "--volumes" ]; then
    VOLUMES_FLAG="--volumes"
    echo "⚠️  Deleting all data including database and media..."
    echo ""
fi

COMPOSE=(docker compose -f docker-compose.oss.yml)
if [ -f "$ENV_OSS_FILE" ]; then
  COMPOSE+=(--env-file "$ENV_OSS_FILE")
fi
"${COMPOSE[@]}" down $VOLUMES_FLAG

echo ""
if [ -z "$VOLUMES_FLAG" ]; then
    echo "✅ SENN stopped (data preserved)"
    echo "   Run ./scripts/oss-up.sh to start again"
else
    echo "✅ SENN stopped and all data deleted"
fi
echo ""
