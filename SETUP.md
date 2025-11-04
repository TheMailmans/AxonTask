# AxonTask Setup Guide

**Version**: 1.0
**Last Updated**: January 3, 2025
**For**: Anyone deploying AxonTask
**Status**: Complete Step-by-Step Guide

---

## Overview

This guide walks you through setting up AxonTask from scratch, whether for local development, testing, or production deployment.

**Time to Complete**: 30-60 minutes

---

## Prerequisites

### Required

- **Rust 1.75+**: [rustup.rs](https://rustup.rs/)
- **Docker & Docker Compose**: [docker.com](https://docs.docker.com/get-docker/)
- **Git**: For cloning repository
- **PostgreSQL client tools** (psql, pg_dump): For database management
- **Redis client** (redis-cli): For debugging

### Optional (for production)

- **Fly.io account** ([fly.io](https://fly.io)) or other hosting platform
- **Domain name** with DNS access
- **Stripe account** (if using billing features)

---

## Part 1: Local Development Setup

### Step 1: Install Rust

```bash
# Install Rust via rustup
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Add to PATH
source $HOME/.cargo/env

# Verify
rustc --version
cargo --version
```

### Step 2: Install sqlx-cli

```bash
cargo install sqlx-cli --no-default-features --features postgres
```

### Step 3: Clone Repository

```bash
git clone https://github.com/TheMailmans/AxonTask.git
cd AxonTask
```

### Step 4: Start Development Services

```bash
# Start PostgreSQL and Redis
docker-compose up -d

# Verify services are running
docker-compose ps
```

Expected output:
```
NAME                  IMAGE               STATUS
axontask-postgres     postgres:15-alpine  Up
axontask-redis        redis:7-alpine      Up
```

### Step 5: Configure Environment

```bash
# Copy example environment file
cp .env.example .env

# Edit with your settings
nano .env
```

**Minimal `.env` for development**:
```bash
DATABASE_URL=postgresql://axontask:axontask@localhost:5432/axontask
REDIS_URL=redis://localhost:6379

JWT_SECRET=dev-secret-change-in-production
HMAC_SECRET=dev-hmac-change-in-production

API_PORT=8080
RUST_LOG=debug

BILLING_ENABLED=false
```

### Step 6: Create Database & Run Migrations

```bash
# Create database
sqlx database create

# Run migrations
sqlx migrate run

# Verify
psql postgresql://axontask:axontask@localhost:5432/axontask -c "\dt"
```

You should see tables: tenants, users, tasks, task_events, etc.

### Step 7: Build Project

```bash
# Build all crates
cargo build

# This will take 5-10 minutes on first build
```

### Step 8: Run Tests

```bash
# Run all tests
cargo test

# All tests should pass
```

### Step 9: Run API Server

```bash
# Terminal 1: Run API
cargo run -p axontask-api

# You should see:
# INFO axontask_api: Server listening on http://127.0.0.1:8080
```

### Step 10: Run Worker

```bash
# Terminal 2: Run Worker
cargo run -p axontask-worker

# You should see:
# INFO axontask_worker: Worker ready and listening for tasks
```

### Step 11: Test API

```bash
# Terminal 3: Test health endpoint
curl http://localhost:8080/health

# Expected: {"status":"healthy","database":"ok","redis":"ok"}

# Register a user
curl -X POST http://localhost:8080/v1/auth/register \
  -H "Content-Type: application/json" \
  -d '{
    "email": "test@example.com",
    "password": "Test123!",
    "name": "Test User",
    "tenant_name": "Test Org"
  }'

# You should get a response with user, tenant, and tokens
```

### Step 12: Create a Test Task

```bash
# Extract access_token from registration response
export TOKEN="<your-access-token>"

# Start a mock task
curl -X POST http://localhost:8080/v1/mcp/start_task \
  -H "Authorization: Bearer $TOKEN" \
  -H "Content-Type: application/json" \
  -d '{
    "name": "test-task",
    "adapter": "mock",
    "args": {},
    "timeout_seconds": 60
  }'

# You should get a task_id and stream_url
```

### Step 13: Stream Task Events

```bash
# Stream events (SSE)
curl -N -H "Authorization: Bearer $TOKEN" \
  http://localhost:8080/v1/mcp/tasks/<task_id>/stream

# You should see events streaming in real-time
```

**✅ Local development setup complete!**

---

## Part 2: Production Deployment

Choose your deployment platform and follow the corresponding guide in [DEPLOYMENT.md](DEPLOYMENT.md):

- [Fly.io + Vercel](DEPLOYMENT.md#flyio-deployment)
- [Railway](DEPLOYMENT.md#railwayrender-deployment)
- [Self-Hosted (Docker)](DEPLOYMENT.md#self-hosted-docker-compose)

---

## Part 3: Frontend Setup (Optional)

### Step 1: Install Node.js

```bash
# Install Node 18+ (use nvm or download from nodejs.org)
nvm install 18
nvm use 18
```

### Step 2: Create Dashboard Project

```bash
# Create Next.js app
npx create-next-app@latest dashboard --typescript --tailwind --app --src-dir

cd dashboard
```

### Step 3: Install Dependencies

```bash
npm install \
  @tanstack/react-query \
  axios \
  next-auth \
  zod \
  react-hook-form \
  @hookform/resolvers \
  zustand \
  recharts \
  @radix-ui/react-dialog \
  @radix-ui/react-dropdown-menu \
  class-variance-authority \
  clsx \
  tailwind-merge
```

### Step 4: Configure Environment

Create `.env.local`:
```bash
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXTAUTH_SECRET=your-nextauth-secret
NEXTAUTH_URL=http://localhost:3000
```

### Step 5: Run Dashboard

```bash
npm run dev
```

Visit `http://localhost:3000`

---

## Part 4: Configuration

### Generating Secrets

```bash
# JWT Secret
openssl rand -base64 32

# HMAC Secret
openssl rand -base64 32

# Ed25519 Signing Key (for receipts)
openssl genpkey -algorithm ed25519 -out private_key.pem
openssl pkey -in private_key.pem -text -noout | grep priv -A 3 | tail -n +2 | tr -d ' :\n'
```

### Setting Up Billing (Optional)

1. **Create Stripe Account**: [stripe.com](https://stripe.com)
2. **Create Products**:
   - Trial (free)
   - Entry ($9.99/month)
   - Pro ($29/month)
   - Enterprise (custom)
3. **Get API Keys**: Dashboard → Developers → API Keys
4. **Set in `.env`**:
   ```bash
   BILLING_ENABLED=true
   STRIPE_SECRET_KEY=sk_test_...
   STRIPE_PUBLISHABLE_KEY=pk_test_...
   ```
5. **Configure Webhooks**:
   - Endpoint: `https://yourdomain.com/v1/billing/webhooks/stripe`
   - Events: `customer.subscription.*`, `invoice.*`
   - Get webhook secret and set `STRIPE_WEBHOOK_SECRET`

### Configuring Adapters

#### Shell Adapter

Already included. Sandboxed by default.

#### Docker Adapter

Requires Docker socket access:
```bash
# Add worker user to docker group (production)
usermod -aG docker axontask-worker
```

#### Fly Adapter

Set Fly.io API token:
```bash
FLY_API_TOKEN=your-token
```

---

## Part 5: Monitoring Setup

### Prometheus

Create `prometheus.yml`:
```yaml
global:
  scrape_interval: 15s

scrape_configs:
  - job_name: 'axontask-api'
    static_configs:
      - targets: ['localhost:8080']
    metrics_path: '/metrics'

  - job_name: 'axontask-worker'
    static_configs:
      - targets: ['localhost:9090']
    metrics_path: '/metrics'
```

Run:
```bash
docker run -d \
  -p 9090:9090 \
  -v $(pwd)/prometheus.yml:/etc/prometheus/prometheus.yml \
  prom/prometheus
```

### Grafana

```bash
docker run -d \
  -p 3000:3000 \
  grafana/grafana
```

Import AxonTask dashboard (JSON in `docs/grafana/`).

---

## Part 6: Database Backup

### Automated Backups

Create `backup-db.sh`:
```bash
#!/bin/bash
DATE=$(date +%Y%m%d_%H%M%S)
BACKUP_DIR="/var/backups/axontask"
mkdir -p $BACKUP_DIR

pg_dump $DATABASE_URL | gzip > $BACKUP_DIR/backup_$DATE.sql.gz

# Keep only last 7 days
find $BACKUP_DIR -name "backup_*.sql.gz" -mtime +7 -delete
```

Make executable and add to cron:
```bash
chmod +x backup-db.sh
crontab -e
# Add: 0 2 * * * /path/to/backup-db.sh
```

---

## Part 7: SSL/TLS Setup

### Using Caddy (Recommended)

Caddy auto-provisions Let's Encrypt certificates.

Install Caddy:
```bash
sudo apt install -y debian-keyring debian-archive-keyring apt-transport-https
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/gpg.key' | sudo gpg --dearmor -o /usr/share/keyrings/caddy-stable-archive-keyring.gpg
curl -1sLf 'https://dl.cloudsmith.io/public/caddy/stable/debian.deb.txt' | sudo tee /etc/apt/sources.list.d/caddy-stable.list
sudo apt update
sudo apt install caddy
```

Configure `/etc/caddy/Caddyfile`:
```
yourdomain.com {
    reverse_proxy localhost:8080
}
```

Restart:
```bash
sudo systemctl restart caddy
```

Caddy automatically gets SSL certificate from Let's Encrypt.

---

## Part 8: Testing Your Setup

### Health Check

```bash
curl https://yourdomain.com/health
```

### End-to-End Test

1. **Register user** via API or dashboard
2. **Create API key** in dashboard
3. **Start a task** via API
4. **Stream events** via SSE
5. **Check task status**
6. **Verify in database**:
   ```sql
   SELECT * FROM tasks ORDER BY created_at DESC LIMIT 5;
   SELECT * FROM task_events WHERE task_id = '<task-id>';
   ```

---

## Part 9: Troubleshooting

### API won't start

**Check database connection**:
```bash
psql $DATABASE_URL -c "SELECT 1"
```

**Check logs**:
```bash
docker-compose logs api
```

**Common issues**:
- `DATABASE_URL` not set
- Migrations not run (`sqlx migrate run`)
- Port 8080 already in use

### Worker not processing tasks

**Check Redis connection**:
```bash
redis-cli -h localhost PING
```

**Check task queue**:
```bash
redis-cli LLEN task_queue
```

**Check worker logs**:
```bash
docker-compose logs worker
```

### Database migration errors

**Reset database** (⚠️ destroys all data):
```bash
sqlx database drop
sqlx database create
sqlx migrate run
```

### SSL certificate not working

**Check DNS**:
```bash
dig yourdomain.com +short
```

**Check Caddy logs**:
```bash
sudo journalctl -u caddy -f
```

---

## Part 10: Next Steps

### Production Checklist

- [ ] Change all secrets (JWT, HMAC, etc.)
- [ ] Enable HTTPS
- [ ] Set up backups (database + Redis)
- [ ] Configure monitoring (Prometheus + Grafana)
- [ ] Set up alerting
- [ ] Test disaster recovery
- [ ] Enable rate limiting
- [ ] Configure CORS for your domain
- [ ] Review security headers
- [ ] Set up log aggregation
- [ ] Document your deployment
- [ ] Test scaling (add more workers)

### Going Live

1. **Test thoroughly** in staging environment
2. **Run load tests** (see `tests/load/`)
3. **Set up monitoring** and alerts
4. **Prepare runbooks** for common issues
5. **Configure backups** and test restoration
6. **Deploy to production**
7. **Monitor closely** for first 24-48 hours

---

## Support

- **Documentation**: See all `.md` files in repo
- **Issues**: [GitHub Issues](https://github.com/TheMailmans/AxonTask/issues)
- **Discussions**: [GitHub Discussions](https://github.com/TheMailmans/AxonTask/discussions)
- **Email**: tyler@axonhub.io

---

**✅ Setup Complete!**

You now have a fully functional AxonTask deployment. For advanced configuration, see:
- [DEPLOYMENT.md](DEPLOYMENT.md) - Deployment platforms
- [DESIGN.md](DESIGN.md) - System architecture
- [DATABASE_DESIGN.md](DATABASE_DESIGN.md) - Database schema
- [API_DESIGN.md](API_DESIGN.md) - API endpoints

---

**Document Version**: 1.0
**Last Updated**: January 3, 2025
**Maintained By**: Tyler Mailman
