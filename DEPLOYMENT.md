# AxonTask Deployment Guide

**Version**: 1.0
**Last Updated**: November 05, 2025
**Status**: Complete Guide for All Platforms

---

## Table of Contents

1. [Deployment Options](#deployment-options)
2. [Fly.io Deployment](#flyio-deployment)
3. [Vercel Deployment (Frontend)](#vercel-deployment-frontend)
4. [Railway/Render Deployment](#railwayrender-deployment)
5. [Self-Hosted (Docker Compose)](#self-hosted-docker-compose)
6. [Environment Variables](#environment-variables)
7. [Database Setup](#database-setup)
8. [SSL/TLS Configuration](#ssltls-configuration)
9. [Monitoring](#monitoring)
10. [Troubleshooting](#troubleshooting)

---

## Deployment Options

| Platform | API + Workers | Frontend | Database | Redis | Best For |
|----------|---------------|----------|----------|-------|----------|
| **Fly.io** | ✅ | ❌ | ✅ (Fly Postgres) | ✅ (Upstash/Fly Redis) | Production, auto-scaling |
| **Vercel** | ❌ | ✅ | ❌ | ❌ | Frontend hosting |
| **Railway** | ✅ | ✅ | ✅ | ✅ | Simpler alternative to Fly |
| **Render** | ✅ | ✅ | ✅ | ✅ | Simpler, managed services |
| **Self-Hosted** | ✅ | ✅ | ✅ | ✅ | Full control, cost-effective |

**Recommended**: Fly.io (API + Workers) + Vercel (Frontend) for production

---

## Fly.io Deployment

### Prerequisites

- Fly.io account ([fly.io](https://fly.io))
- `flyctl` CLI installed
- Docker installed (for building images)

### Step 1: Install flyctl

```bash
# macOS
brew install flyctl

# Linux
curl -L https://fly.io/install.sh | sh

# Login
flyctl auth login
```

### Step 2: Create Fly App (API)

```bash
cd axontask-api
flyctl launch --name axontask-api --region iad --no-deploy
```

Edit `fly.toml`:

```toml
app = "axontask-api"
primary_region = "iad"

[build]
  dockerfile = "Dockerfile.api"

[env]
  PORT = "8080"
  RUST_LOG = "info"

[[services]]
  internal_port = 8080
  protocol = "tcp"

  [[services.ports]]
    handlers = ["http"]
    port = 80
  
  [[services.ports]]
    handlers = ["tls", "http"]
    port = 443

  [services.concurrency]
    hard_limit = 1000
    soft_limit = 800

[[vm]]
  cpu_kind = "shared"
  cpus = 1
  memory_mb = 512
```

### Step 3: Create Postgres Database

```bash
flyctl postgres create --name axontask-db --region iad
flyctl postgres attach --app axontask-api axontask-db
```

This automatically sets `DATABASE_URL` secret.

### Step 4: Create Redis (Upstash)

```bash
# Create Upstash Redis at https://upstash.com
# Get connection URL

flyctl secrets set REDIS_URL="redis://..." --app axontask-api
```

### Step 5: Set Secrets

```bash
flyctl secrets set \
  JWT_SECRET="$(openssl rand -base64 32)" \
  HMAC_SECRET="$(openssl rand -base64 32)" \
  --app axontask-api
```

### Step 6: Deploy API

```bash
flyctl deploy --app axontask-api
```

### Step 7: Deploy Worker

```bash
cd ../axontask-worker
flyctl launch --name axontask-worker --region iad --no-deploy
```

Edit `fly.toml` (same DATABASE_URL, REDIS_URL from API):

```toml
app = "axontask-worker"
primary_region = "iad"

[build]
  dockerfile = "Dockerfile.worker"

[env]
  RUST_LOG = "info"

# No services block (worker doesn't expose HTTP)
```

Attach same database:

```bash
flyctl postgres attach --app axontask-worker axontask-db
flyctl secrets import --app axontask-worker < api-secrets.txt
```

Deploy:

```bash
flyctl deploy --app axontask-worker
```

### Step 8: Auto-Scaling

```bash
# Scale API based on CPU
flyctl autoscale set min=1 max=10 --app axontask-api

# Scale Worker based on queue depth (manual scaling for now)
flyctl scale count 3 --app axontask-worker
```

### Step 9: Monitor

```bash
flyctl logs --app axontask-api
flyctl status --app axontask-api
flyctl dashboard --app axontask-api
```

---

## Vercel Deployment (Frontend)

### Prerequisites

- Vercel account
- Vercel CLI installed

### Step 1: Install Vercel CLI

```bash
npm i -g vercel
```

### Step 2: Configure Project

Create `vercel.json` in dashboard root:

```json
{
  "buildCommand": "npm run build",
  "devCommand": "npm run dev",
  "installCommand": "npm install",
  "framework": "nextjs",
  "outputDirectory": ".next",
  "env": {
    "NEXT_PUBLIC_API_URL": "https://axontask-api.fly.dev"
  }
}
```

### Step 3: Deploy

```bash
cd dashboard
vercel
```

Follow prompts:
- Link to existing project or create new
- Set environment variables
- Deploy

### Step 4: Set Environment Variables

In Vercel Dashboard → Settings → Environment Variables:

```
NEXT_PUBLIC_API_URL=https://axontask-api.fly.dev
NEXTAUTH_SECRET=<generate-secret>
NEXTAUTH_URL=https://yourdomain.com
```

### Step 5: Custom Domain

Vercel Dashboard → Settings → Domains → Add domain

---

## Railway/Render Deployment

### Railway

1. **Sign up**: [railway.app](https://railway.app)
2. **New Project** → Deploy from GitHub
3. **Select repo**: TheMailmans/AxonTask
4. **Add PostgreSQL**: Railway marketplace
5. **Add Redis**: Railway marketplace
6. **Add API service**: Detect Dockerfile.api
7. **Add Worker service**: Detect Dockerfile.worker
8. **Add Frontend**: Detect package.json (dashboard/)
9. **Set environment variables** in each service
10. **Deploy**

### Render

1. **Sign up**: [render.com](https://render.com)
2. **New Blueprint**
3. **Connect GitHub repo**
4. Create `render.yaml`:

```yaml
services:
  - type: web
    name: axontask-api
    env: docker
    dockerfilePath: ./Dockerfile.api
    envVars:
      - key: DATABASE_URL
        fromDatabase:
          name: axontask-db
          property: connectionString
      - key: REDIS_URL
        fromService:
          name: axontask-redis
          property: connectionString
  
  - type: worker
    name: axontask-worker
    env: docker
    dockerfilePath: ./Dockerfile.worker
    envVars:
      - key: DATABASE_URL
        fromDatabase:
          name: axontask-db
          property: connectionString
  
  - type: web
    name: axontask-dashboard
    env: static
    buildCommand: cd dashboard && npm install && npm run build
    staticPublishPath: ./dashboard/out
    envVars:
      - key: NEXT_PUBLIC_API_URL
        value: https://axontask-api.onrender.com

databases:
  - name: axontask-db
    databaseName: axontask
    user: axontask

  - name: axontask-redis
    plan: starter
```

5. **Commit & Push** → Auto-deploy

---

## Self-Hosted (Docker Compose)

### Prerequisites

- VPS (DigitalOcean, Hetzner, Linode)
- Ubuntu 22.04+
- Docker & Docker Compose
- Domain with DNS pointing to server

### Step 1: Server Setup

```bash
# SSH into server
ssh root@your-server-ip

# Install Docker
curl -fsSL https://get.docker.com | sh

# Install Docker Compose
apt install docker-compose-plugin

# Create user
adduser axontask
usermod -aG docker axontask
su - axontask
```

### Step 2: Clone Repository

```bash
git clone https://github.com/TheMailmans/AxonTask.git
cd AxonTask
```

### Step 3: Configure Environment

```bash
cp .env.example .env
nano .env
```

Edit all required values (see [Environment Variables](#environment-variables)).

### Step 4: Production Docker Compose

Create `docker-compose.prod.yml`:

```yaml
version: '3.8'

services:
  postgres:
    image: postgres:15-alpine
    restart: always
    volumes:
      - postgres_data:/var/lib/postgresql/data
    environment:
      POSTGRES_DB: ${POSTGRES_DB:-axontask}
      POSTGRES_USER: ${POSTGRES_USER:-axontask}
      POSTGRES_PASSWORD: ${POSTGRES_PASSWORD}
    healthcheck:
      test: ["CMD", "pg_isready", "-U", "axontask"]
      interval: 10s
      timeout: 5s
      retries: 5

  redis:
    image: redis:7-alpine
    restart: always
    command: redis-server --appendonly yes --requirepass ${REDIS_PASSWORD}
    volumes:
      - redis_data:/data
    healthcheck:
      test: ["CMD", "redis-cli", "--raw", "incr", "ping"]
      interval: 10s
      timeout: 5s
      retries: 5

  api:
    build:
      context: .
      dockerfile: Dockerfile.api
    restart: always
    ports:
      - "8080:8080"
    env_file: .env
    environment:
      DATABASE_URL: postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB}
      REDIS_URL: redis://:${REDIS_PASSWORD}@redis:6379
    depends_on:
      - postgres
      - redis
    healthcheck:
      test: ["CMD", "curl", "-f", "http://localhost:8080/health"]
      interval: 30s
      timeout: 10s
      retries: 3

  worker:
    build:
      context: .
      dockerfile: Dockerfile.worker
    restart: always
    env_file: .env
    environment:
      DATABASE_URL: postgresql://${POSTGRES_USER}:${POSTGRES_PASSWORD}@postgres:5432/${POSTGRES_DB}
      REDIS_URL: redis://:${REDIS_PASSWORD}@redis:6379
    depends_on:
      - postgres
      - redis
    deploy:
      replicas: 2

  caddy:
    image: caddy:2-alpine
    restart: always
    ports:
      - "80:80"
      - "443:443"
    volumes:
      - ./Caddyfile:/etc/caddy/Caddyfile
      - caddy_data:/data
      - caddy_config:/config
    depends_on:
      - api

volumes:
  postgres_data:
  redis_data:
  caddy_data:
  caddy_config:
```

### Step 5: Caddy Configuration

Create `Caddyfile`:

```
yourdomain.com {
    reverse_proxy api:8080
    
    log {
        output file /var/log/caddy/access.log
    }
    
    header {
        # Security headers
        Strict-Transport-Security "max-age=31536000; includeSubDomains; preload"
        X-Content-Type-Options "nosniff"
        X-Frame-Options "SAMEORIGIN"
        X-XSS-Protection "1; mode=block"
        Referrer-Policy "strict-origin-when-cross-origin"
    }
}
```

### Step 6: Deploy

```bash
docker compose -f docker-compose.prod.yml up -d
```

### Step 7: Run Migrations

```bash
docker compose exec api sqlx migrate run
```

### Step 8: Monitor

```bash
# Logs
docker compose logs -f api
docker compose logs -f worker

# Status
docker compose ps
```

---

## Environment Variables

### Required

| Variable | Description | Example |
|----------|-------------|---------|
| `DATABASE_URL` | PostgreSQL connection string | `postgresql://user:pass@host:5432/db` |
| `REDIS_URL` | Redis connection string | `redis://localhost:6379` |
| `JWT_SECRET` | JWT signing secret | `openssl rand -base64 32` |
| `HMAC_SECRET` | Webhook signature secret | `openssl rand -base64 32` |
| `API_PORT` | API server port | `8080` |

### Optional

| Variable | Default | Description |
|----------|---------|-------------|
| `RUST_LOG` | `info` | Log level |
| `BILLING_ENABLED` | `false` | Enable Stripe billing |
| `STRIPE_SECRET_KEY` | | Stripe secret key |
| `CORS_ALLOWED_ORIGINS` | `*` | CORS origins (comma-separated) |

---

## Database Setup

### Migrations

```bash
# Create database
sqlx database create

# Run migrations
sqlx migrate run

# Verify
psql $DATABASE_URL -c "SELECT COUNT(*) FROM tasks;"
```

### Backups

```bash
# Manual backup
pg_dump $DATABASE_URL | gzip > backup-$(date +%Y%m%d).sql.gz

# Automated (cron)
0 2 * * * /usr/local/bin/backup-db.sh
```

---

## SSL/TLS Configuration

### Let's Encrypt (Caddy - Auto)

Caddy automatically provisions SSL certificates. No configuration needed.

### Let's Encrypt (Certbot - Manual)

```bash
apt install certbot
certbot certonly --standalone -d yourdomain.com
```

Update Nginx/Caddy config to use certificates.

---

## Monitoring

### Prometheus + Grafana

```yaml
# Add to docker-compose.prod.yml
prometheus:
  image: prom/prometheus
  volumes:
    - ./prometheus.yml:/etc/prometheus/prometheus.yml
  ports:
    - "9090:9090"

grafana:
  image: grafana/grafana
  ports:
    - "3000:3000"
  volumes:
    - grafana_data:/var/lib/grafana
```

### Health Checks

```bash
curl http://localhost:8080/health
# {"status":"healthy","database":"ok","redis":"ok"}
```

---

## Troubleshooting

### API Won't Start

```bash
# Check logs
docker compose logs api

# Common issues:
# - DATABASE_URL not set
# - Database migrations not run
# - Redis not accessible
```

### Worker Not Processing Tasks

```bash
# Check Redis connection
redis-cli -h localhost ping

# Check queue
redis-cli LLEN task_queue

# Check worker logs
docker compose logs worker
```

### SSL Certificate Issues

```bash
# Renew Certbot
certbot renew

# Check Caddy logs
docker compose logs caddy
```

---

**Document Version**: 1.0
**Last Updated**: November 05, 2025
**Maintained By**: Tyler Mailman
