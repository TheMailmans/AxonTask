#!/bin/bash
# Integration test runner for AxonTask
#
# This script:
# 1. Starts test database and Redis containers
# 2. Runs database migrations
# 3. Runs integration tests
# 4. Cleans up containers (optional)

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}🚀 AxonTask Integration Test Runner${NC}\n"

# Check if Docker is running
if ! docker info > /dev/null 2>&1; then
    echo -e "${RED}❌ Docker is not running. Please start Docker and try again.${NC}"
    exit 1
fi

# Start test containers
echo -e "${YELLOW}📦 Starting test containers...${NC}"
docker-compose -f docker-compose.test.yml up -d

# Wait for services to be healthy
echo -e "${YELLOW}⏳ Waiting for services to be ready...${NC}"
sleep 5

# Check PostgreSQL health
echo -e "${YELLOW}🔍 Checking PostgreSQL...${NC}"
until docker-compose -f docker-compose.test.yml exec -T postgres-test pg_isready -U axontask_test > /dev/null 2>&1; do
    echo "  Waiting for PostgreSQL..."
    sleep 2
done
echo -e "${GREEN}  ✓ PostgreSQL ready${NC}"

# Check Redis health
echo -e "${YELLOW}🔍 Checking Redis...${NC}"
until docker-compose -f docker-compose.test.yml exec -T redis-test redis-cli ping > /dev/null 2>&1; do
    echo "  Waiting for Redis..."
    sleep 2
done
echo -e "${GREEN}  ✓ Redis ready${NC}"

# Load test environment
echo -e "${YELLOW}📝 Loading test environment...${NC}"
export $(cat .env.test | grep -v '^#' | xargs)

# Run migrations
echo -e "${YELLOW}🔄 Running database migrations...${NC}"
sqlx migrate run --source ./migrations

# Run tests
echo -e "\n${GREEN}🧪 Running integration tests...${NC}\n"
cargo test --test integration_test -- --nocapture

TEST_EXIT_CODE=$?

# Cleanup (optional - comment out to keep containers running for debugging)
if [ "$1" != "--keep" ]; then
    echo -e "\n${YELLOW}🧹 Cleaning up test containers...${NC}"
    docker-compose -f docker-compose.test.yml down
fi

# Report results
echo ""
if [ $TEST_EXIT_CODE -eq 0 ]; then
    echo -e "${GREEN}✅ All tests passed!${NC}"
else
    echo -e "${RED}❌ Some tests failed.${NC}"
    if [ "$1" != "--keep" ]; then
        echo -e "${YELLOW}💡 Tip: Run with --keep flag to keep containers running for debugging${NC}"
    fi
fi

exit $TEST_EXIT_CODE
