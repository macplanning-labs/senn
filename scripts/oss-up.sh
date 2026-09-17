#!/bin/bash
# ============================================================
# SENN OSS Self-host Startup Script
# 
# Usage:
#   ./scripts/oss-up.sh
#
# This script:
# 1. Creates .env.oss from .env.oss.example (if not exists)
# 2. Fills in random secrets using openssl
# 3. Starts all services with docker-compose
# 4. Waits for services to be healthy
# 5. Prints the access URL
# ============================================================

set -e

PROJECT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$PROJECT_DIR"

ENV_OSS_FILE=".env.oss"
ENV_OSS_EXAMPLE=".env.oss.example"

echo "🚀 SENN OSS Self-host Startup"
echo "================================"
echo ""

# Step 1: Create .env.oss if it doesn't exist
if [ ! -f "$ENV_OSS_FILE" ]; then
    echo "📝 Creating $ENV_OSS_FILE from $ENV_OSS_EXAMPLE..."
    cp "$ENV_OSS_EXAMPLE" "$ENV_OSS_FILE"
    
    # Generate random secrets (do NOT echo them)
    DB_PASSWORD=$(openssl rand -base64 24 | tr -d '\n')
    SESSION_SECRET=$(openssl rand -base64 32 | tr -d '\n')
    DJANGO_SECRET_KEY=$(openssl rand -base64 32 | tr -d '\n')
    
    # Update the file with random secrets using temp file (safer than sed with special chars)
    {
        echo "DB_PASSWORD=$DB_PASSWORD"
        echo "POSTGRES_DB=qa_tool"
        echo "POSTGRES_USER=qa_admin"
        echo ""
        echo "SESSION_SECRET=$SESSION_SECRET"
        echo "DJANGO_SECRET_KEY=$DJANGO_SECRET_KEY"
    } > "$ENV_OSS_FILE"
    
    echo "✅ Generated random secrets in $ENV_OSS_FILE"
else
    echo "✅ Using existing $ENV_OSS_FILE"
fi

echo ""

# Step 2: Start services
echo "🐳 Starting Docker services..."
docker compose -f docker-compose.oss.yml --env-file "$ENV_OSS_FILE" up --build -d

echo ""
echo "⏳ Waiting for services to be healthy..."

# Wait for db healthcheck
max_attempts=30
attempt=0
while [ $attempt -lt $max_attempts ]; do
    if docker compose -f docker-compose.oss.yml --env-file "$ENV_OSS_FILE" ps db | grep -q "healthy"; then
        echo "✅ Database is healthy"
        break
    fi
    attempt=$((attempt + 1))
    echo "   DB attempt $attempt/$max_attempts..."
    sleep 2
done

# Wait for rust to start
echo "⏳ Waiting for API server..."
sleep 3

# Wait for web service to respond with HTTP 200
echo "⏳ Waiting for web interface..."
max_attempts=40
attempt=0
while [ $attempt -lt $max_attempts ]; do
    http_code=$(curl -sS -o /dev/null -w '%{http_code}' http://localhost:8080/ 2>/dev/null || echo "000")
    if [ "$http_code" = "200" ]; then
        echo "✅ Web interface is responding (HTTP $http_code)"
        break
    fi
    attempt=$((attempt + 1))
    if [ $((attempt % 5)) -eq 0 ]; then
        echo "   Web attempt $attempt/$max_attempts (HTTP $http_code)..."
    fi
    sleep 1
done

# Verify API is accessible
echo "⏳ Verifying API..."
api_code=$(curl -sS -o /dev/null -w '%{http_code}' http://localhost:8080/api/v1/auth/ 2>/dev/null || echo "000")
if [ "$api_code" = "401" ] || [ "$api_code" = "200" ]; then
    echo "✅ API is accessible (HTTP $api_code)"
else
    echo "⚠️  API returned HTTP $api_code (may still be starting)"
fi

echo ""
echo "================================"
echo "✅ SENN is ready!"
echo ""
echo "🌐 Access SENN at:"
echo "   http://localhost:8080"
echo ""
echo "📝 Next steps:"
echo "   1. Register a new account"
echo "   2. Create a Team (/teams)"
echo "   3. Create a Ticket in the Team"
echo ""
echo "🛑 To stop:"
echo "   ./scripts/oss-down.sh"
echo ""
