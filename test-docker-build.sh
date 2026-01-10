#!/bin/bash
# Test script for tark Docker build
# Run this on your local machine with Docker installed

set -e

echo "=== tark Docker Build Test ==="
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check Docker is available
if ! command -v docker &> /dev/null; then
    echo -e "${RED}Error: Docker is not installed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ Docker is available${NC}"

# Get script directory (where the Dockerfiles are)
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

echo ""
echo "Building from: $SCRIPT_DIR"
echo ""

# Clean up old containers/images
echo "Cleaning up old containers..."
docker rm -f tark-server 2>/dev/null || true
docker rmi tark:local-alpine 2>/dev/null || true

# Build Alpine image
echo ""
echo -e "${YELLOW}Building Alpine image (this takes 3-5 minutes on first run)...${NC}"
echo ""

if docker build -f Dockerfile.alpine -t tark:local-alpine .; then
    echo ""
    echo -e "${GREEN}✓ Build successful!${NC}"
else
    echo ""
    echo -e "${RED}✗ Build failed${NC}"
    exit 1
fi

# Show image size
echo ""
echo "Image size:"
docker images tark:local-alpine --format "{{.Repository}}:{{.Tag}} - {{.Size}}"

# Test run
echo ""
echo "Starting container..."
docker run -d --name tark-server -p 8765:8765 \
    -e OPENAI_API_KEY="${OPENAI_API_KEY:-}" \
    -e ANTHROPIC_API_KEY="${ANTHROPIC_API_KEY:-}" \
    tark:local-alpine

# Wait for startup
echo "Waiting for server to start..."
sleep 3

# Health check
echo ""
echo "Testing health endpoint..."
if curl -sf http://localhost:8765/health; then
    echo ""
    echo -e "${GREEN}✓ Server is running!${NC}"
else
    echo ""
    echo -e "${RED}✗ Health check failed${NC}"
    echo ""
    echo "Container logs:"
    docker logs tark-server
fi

# Show logs
echo ""
echo "Container logs:"
docker logs tark-server

# Cleanup
echo ""
echo "Cleaning up..."
docker stop tark-server
docker rm tark-server

echo ""
echo -e "${GREEN}=== Test Complete ===${NC}"
echo ""
echo "To use in Neovim, run :Lazy sync and restart Neovim"

