# AxonTask

**Persistent Background Tasks & Streaming for AI Agents**

[![License: BSL](https://img.shields.io/badge/License-BSL%201.1-blue.svg)](LICENSE-BSL.md)
[![Rust](https://img.shields.io/badge/rust-1.75%2B-orange.svg)](https://www.rust-lang.org/)
[![Status](https://img.shields.io/badge/status-phase%200%20complete-green.svg)](https://github.com/TheMailmans/AxonTask)
[![Commercial License](https://img.shields.io/badge/Commercial-Available-green.svg)](COMMERCIAL_LICENSE.md)

> **"Agents lie when they say 'I'll update you.' AxonTask makes that true."**

AxonTask is a production-ready, open-source system that enables AI agents to start long-running background tasks, stream progress in real-time via Server-Sent Events (SSE), and resume reliably across session interruptions.

**Status**: ✅ Phase 0 Complete - Foundation Ready

---

## Features

### Core Capabilities

- 🔄 **Persistent Execution**: Tasks survive agent restarts, browser reloads, and server deployments
- 📡 **Real-Time Streaming**: Live progress via SSE with automatic reconnection
- 🔁 **Resumable from Any Point**: Durable replay with Redis Streams (XREAD backfill + live tail)
- 🔐 **Hash-Chained Events**: Tamper-evident audit trail with optional signed receipts
- 🛡️ **Production Security**: JWT + API keys, rate limiting, quota enforcement, sandbox execution
- 🎯 **Self-Hostable**: Fully open-source with optional Stripe billing for SaaS

### MCP-Native Tools

AxonTask provides Model Context Protocol (MCP) tools for seamless agent integration:

- `start_task`: Start a background task (shell, docker, fly, etc.)
- `stream_task`: Stream live events via SSE
- `get_task_status`: Check task state and progress
- `resume_task`: Reconnect and resume from last position
- `cancel_task`: Cancel a running task
- `get_task_receipt`: Download integrity receipt (hash chain + signature)

### Adapters

Execute tasks via multiple adapters:

- **Mock**: Deterministic fake events for testing/demos
- **Shell**: Sandboxed command execution (seccomp, cgroups)
- **Docker**: Build/run containers with log streaming
- **Fly.io**: Monitor deployments and rollouts

---

## Architecture

```
┌──────────────┐
│   AI Agents  │  (Claude, GPT, custom agents)
│  (MCP Tools) │
└──────┬───────┘
       │ HTTP/SSE
       ▼
┌──────────────────────────────┐
│  Axum API Server (Rust)      │
│  - MCP endpoints             │
│  - Auth (JWT + API keys)     │
│  - Rate limiting & quotas    │
│  - SSE streaming             │
└──────┬───────────────────────┘
       │
       ├─────► PostgreSQL (tasks, events, users)
       ├─────► Redis Streams (event fanout, replay)
       │
       ▼
┌──────────────────────────────┐
│  Tokio Workers (Rust)        │
│  - Execute tasks via adapters│
│  - Emit hash-chained events  │
│  - Heartbeats & reclaim      │
└──────────────────────────────┘
```

**See [CLAUDE.md](CLAUDE.md) for detailed architecture and development guide.**

---

## Quick Start

### Prerequisites

- **Rust**: 1.75 or later
- **Docker & Docker Compose**: For PostgreSQL and Redis
- **sqlx-cli**: `cargo install sqlx-cli --no-default-features --features postgres`

### Installation

```bash
# Clone the repository
git clone https://github.com/TheMailmans/AxonTask.git
cd AxonTask

# Start development services
docker-compose up -d

# Set up environment
cp .env.example .env
# Edit .env with your configuration

# Run migrations
sqlx database create
sqlx migrate run

# Build the project
cargo build

# Run tests
cargo test
```

### Running Locally

```bash
# Terminal 1: Run API server
cargo run -p axontask-api

# Terminal 2: Run worker
cargo run -p axontask-worker
```

The API will be available at `http://localhost:8080`.

**Note**: Current implementation is Phase 0 (foundation). Full functionality will be available as development progresses. See [ROADMAP.md](ROADMAP.md) for details.

---

## Documentation

### Development & Architecture
- **[ROADMAP.md](ROADMAP.md)**: Complete development roadmap with 16 phases
- **[CLAUDE.md](CLAUDE.md)**: Architecture guide for Claude Code and developers
- **[DESIGN.md](DESIGN.md)**: Complete system design and architecture
- **[DATABASE_DESIGN.md](DATABASE_DESIGN.md)**: Database schema, tables, and indexes
- **[API_DESIGN.md](API_DESIGN.md)**: Complete API specification with all endpoints
- **[FRONTEND_DESIGN.md](FRONTEND_DESIGN.md)**: Dashboard UI design and components

### Deployment & Setup
- **[SETUP.md](SETUP.md)**: Step-by-step setup guide for any platform
- **[DEPLOYMENT.md](DEPLOYMENT.md)**: Deployment guides for Fly.io, Vercel, Railway, and self-hosting

### Contribution & Licensing
- **[CONTRIBUTING.md](CONTRIBUTING.md)**: Contribution guidelines and code standards
- **[CLA.md](CLA.md)**: Contributor License Agreement
- **[LICENSE-BSL.md](LICENSE-BSL.md)**: Business Source License
- **[COMMERCIAL_LICENSE.md](COMMERCIAL_LICENSE.md)**: Commercial licensing options

### Key Concepts

- **MCP Tools**: Model Context Protocol endpoints for agent integration
- **Redis Streams**: Durable event replay with XREAD (backfill + live tail)
- **Hash Chain**: Each event includes SHA-256 hash of previous event (tamper-evident)
- **Adapters**: Pluggable task execution (shell, docker, fly, custom)
- **Tenant Isolation**: Every query filters by tenant_id (multi-tenancy built-in)

---

## Development

### Commands

```bash
# Run all tests
cargo test

# Run specific crate tests
cargo test -p axontask-api
cargo test -p axontask-worker
cargo test -p axontask-shared

# Format code
cargo fmt --all

# Run linter
cargo clippy --all-targets --all-features -- -D warnings

# Generate documentation
cargo doc --open --no-deps

# Run with logs
RUST_LOG=debug cargo run -p axontask-api
```

### Project Structure

```
axontask/
├── axontask-api/         # Axum API server (MCP endpoints, auth, streaming)
├── axontask-worker/      # Tokio worker (task execution, adapters)
├── axontask-shared/      # Shared types, models, utilities
├── migrations/           # Database migrations (sqlx)
├── docs/                 # Documentation
├── tests/                # Integration and load tests
└── docker-compose.yml    # Local development environment
```

---

## Project Status

AxonTask Phase 0 (Foundation) is complete and production-ready:

- ✅ Complete database layer with 11 tables and full CRUD operations
- ✅ Authentication system (JWT + API keys)
- ✅ Rate limiting and quota enforcement
- ✅ Redis Streams infrastructure for durable event replay
- ✅ Cargo workspace (3 crates: api, worker, shared)
- ✅ Docker Compose development environment
- ✅ 177 tests passing (169 unit + 8 integration)
- ✅ Zero technical debt
- ✅ Comprehensive documentation (13 files)

See [CONTRIBUTING.md](CONTRIBUTING.md) for how to contribute bug fixes, documentation improvements, and community-requested features.

---

## Contributing

We welcome contributions! Please read [CONTRIBUTING.md](CONTRIBUTING.md) for:

- Code standards and style guide
- Testing requirements (>80% coverage)
- Commit message format
- Pull request process
- Zero technical debt policy

### Development Principles

- **No shortcuts**: Production-grade code from day one
- **Self-documenting**: Clear naming, comprehensive doc comments
- **Test-driven**: All features have tests before merge
- **Security-first**: Never log secrets, enforce tenant isolation, validate input

---

## License

AxonTask is licensed under the **Business Source License 1.1 (BSL)**. See [LICENSE-BSL.md](LICENSE-BSL.md) for complete terms.

### What This Means

**✅ You CAN use AxonTask for FREE if:**
- Personal, educational, or hobby projects
- Open-source projects (that aren't commercial services)
- Internal use in companies with <$1M revenue or <25 employees
- Development, testing, and evaluation

**❌ You NEED a Commercial License if:**
- Running as a paid SaaS or managed service
- Embedding in a commercial product you sell
- Company has >$1M revenue or >25 employees (internal use)
- Reselling or white-labeling AxonTask

**🔄 Future Open Source Conversion:**
- On January 1, 2035 (10 years), this code converts to **Apache 2.0** license
- Becomes fully open source and permissive

### Commercial Licensing

Need a commercial license? We offer three tiers:

- **Starter**: $499/month - For startups and small SaaS companies
- **Professional**: $1,499/month - For growing businesses with white-labeling
- **Enterprise**: Custom pricing - For large companies and resellers

See [COMMERCIAL_LICENSE.md](COMMERCIAL_LICENSE.md) for complete details and pricing.

**Contact**: themailmaninbox@gmail.com for commercial licensing inquiries.

---

## Support

- **Documentation**: See [docs/](docs/) and [CLAUDE.md](CLAUDE.md)
- **Issues**: [GitHub Issues](https://github.com/TheMailmans/AxonTask/issues)
- **Discussions**: [GitHub Discussions](https://github.com/TheMailmans/AxonTask/discussions)

---

## Acknowledgments

Built with:
- [Rust](https://www.rust-lang.org/) - Systems programming language
- [Axum](https://github.com/tokio-rs/axum) - Web framework
- [Tokio](https://tokio.rs/) - Async runtime
- [PostgreSQL](https://www.postgresql.org/) - Database
- [Redis](https://redis.io/) - Streams and caching
- [sqlx](https://github.com/launchbadge/sqlx) - SQL toolkit

---

**Made with ❤️ for the AI agent ecosystem**
