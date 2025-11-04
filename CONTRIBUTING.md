# Contributing to AxonTask

Thank you for your interest in contributing to AxonTask! This document provides guidelines and standards for contributing to the project.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Workflow](#development-workflow)
- [Code Standards](#code-standards)
- [Testing Requirements](#testing-requirements)
- [Documentation Standards](#documentation-standards)
- [Commit Guidelines](#commit-guidelines)
- [Pull Request Process](#pull-request-process)
- [Review Process](#review-process)

---

## Code of Conduct

### Our Standards

- **Be respectful**: Treat all contributors with respect and professionalism
- **Be constructive**: Provide helpful feedback and suggestions
- **Be inclusive**: Welcome contributors of all skill levels and backgrounds
- **Be collaborative**: Work together to improve the project

### Unacceptable Behavior

- Harassment, discrimination, or personal attacks
- Trolling, insulting comments, or unconstructive criticism
- Publishing others' private information without permission
- Any conduct that would be inappropriate in a professional setting

---

## Getting Started

### Prerequisites

- **Rust**: 1.75 or later (`rustup install stable`)
- **Docker**: For running PostgreSQL and Redis
- **Git**: For version control
- **sqlx-cli**: For database migrations (`cargo install sqlx-cli --no-default-features --features postgres`)

### Initial Setup

1. **Fork the repository** on GitHub

2. **Clone your fork**:
   ```bash
   git clone https://github.com/YOUR_USERNAME/axontask.git
   cd axontask
   ```

3. **Add upstream remote**:
   ```bash
   git remote add upstream https://github.com/ORIGINAL_OWNER/axontask.git
   ```

4. **Start development services**:
   ```bash
   docker-compose up -d
   ```

5. **Set up environment**:
   ```bash
   cp .env.example .env
   # Edit .env with your configuration
   ```

6. **Run migrations**:
   ```bash
   sqlx database create
   sqlx migrate run
   ```

7. **Build the project**:
   ```bash
   cargo build
   ```

8. **Run tests**:
   ```bash
   cargo test
   ```

---

## Development Workflow

### 1. Create a Branch

Always create a new branch for your work:

```bash
git checkout -b feature/your-feature-name
```

**Branch Naming Conventions**:
- `feature/` - New features (e.g., `feature/add-webhook-retry`)
- `fix/` - Bug fixes (e.g., `fix/handle-rate-limit-error`)
- `docs/` - Documentation updates (e.g., `docs/update-api-reference`)
- `refactor/` - Code refactoring (e.g., `refactor/extract-auth-middleware`)
- `test/` - Test additions/improvements (e.g., `test/add-integration-tests`)
- `chore/` - Maintenance tasks (e.g., `chore/update-dependencies`)

### 2. Make Your Changes

Follow the [Code Standards](#code-standards) and [Testing Requirements](#testing-requirements).

### 3. Commit Your Changes

Follow the [Commit Guidelines](#commit-guidelines).

### 4. Keep Your Branch Updated

Regularly sync with upstream:

```bash
git fetch upstream
git rebase upstream/main
```

### 5. Push Your Changes

```bash
git push origin feature/your-feature-name
```

### 6. Open a Pull Request

See [Pull Request Process](#pull-request-process) for details.

---

## Code Standards

### Zero Technical Debt Policy

**We maintain production-grade code from day one. No exceptions.**

#### ❌ Never Allowed

- `TODO` comments (create GitHub issues instead)
- Placeholder implementations (e.g., `unimplemented!()` in non-test code)
- Hardcoded values (use configuration or constants)
- Copy-pasted code (extract to shared functions)
- Dead code (unused functions, imports, etc.)
- Commented-out code (use Git history instead)

#### ✅ Always Required

- Clean, self-documenting code
- Proper error handling (no `.unwrap()` except in tests)
- Comprehensive tests (see [Testing Requirements](#testing-requirements))
- Complete documentation (see [Documentation Standards](#documentation-standards))

### Rust Code Style

#### Formatting

**All code must be formatted with `rustfmt`**:

```bash
cargo fmt --all
```

We use the default `rustfmt` configuration. CI will fail if code is not formatted.

#### Linting

**All code must pass `clippy` in strict mode**:

```bash
cargo clippy --all-targets --all-features -- -D warnings
```

This treats all warnings as errors. Fix all clippy warnings before committing.

#### Naming Conventions

- **Types**: `PascalCase` (e.g., `TaskEvent`, `ApiKey`)
- **Functions**: `snake_case` (e.g., `create_task`, `validate_token`)
- **Constants**: `SCREAMING_SNAKE_CASE` (e.g., `MAX_RETRIES`, `DEFAULT_TIMEOUT`)
- **Modules**: `snake_case` (e.g., `auth`, `rate_limit`)
- **Traits**: Descriptive nouns or adjectives (e.g., `Adapter`, `Authenticator`)

#### Error Handling

**Use `Result<T, E>` for all fallible operations**:

```rust
// ✅ Good
pub fn create_task(name: &str) -> Result<Task, CreateTaskError> {
    if name.is_empty() {
        return Err(CreateTaskError::EmptyName);
    }
    // ...
}

// ❌ Bad
pub fn create_task(name: &str) -> Task {
    assert!(!name.is_empty());  // Panics are not acceptable
    // ...
}
```

**Use custom error types with `thiserror`**:

```rust
use thiserror::Error;

#[derive(Error, Debug)]
pub enum CreateTaskError {
    #[error("task name cannot be empty")]
    EmptyName,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
}
```

**Never use `.unwrap()` or `.expect()` in production code**:

```rust
// ✅ Good
let task = task_repo.find(id).await?;

// ❌ Bad
let task = task_repo.find(id).await.unwrap();
```

**Exception**: `.unwrap()` is acceptable in tests and when you have proven invariants.

#### Async/Await

**Use async/await consistently**:

```rust
// ✅ Good
pub async fn create_task(&self, task: Task) -> Result<Task> {
    self.repo.insert(task).await?;
    Ok(task)
}

// ❌ Bad - mixing sync and async
pub fn create_task(&self, task: Task) -> Result<Task> {
    block_on(self.repo.insert(task))?;  // Don't block in async context
    Ok(task)
}
```

#### Ownership and Borrowing

**Prefer borrowing over cloning when possible**:

```rust
// ✅ Good
pub fn validate_name(name: &str) -> bool {
    !name.is_empty()
}

// ❌ Bad - unnecessary clone
pub fn validate_name(name: String) -> bool {
    !name.is_empty()
}
```

**Clone only when necessary** (e.g., moving data into async closures):

```rust
// ✅ Good - clone needed for async move
let name = task.name.clone();
tokio::spawn(async move {
    process_task(name).await;
});
```

### Database Patterns

#### Always Use `sqlx!` Macros

**Use `query!()` and `query_as!()` for compile-time query checking**:

```rust
// ✅ Good - compile-time checked
let task = sqlx::query_as!(
    Task,
    "SELECT * FROM tasks WHERE id = $1 AND tenant_id = $2",
    task_id,
    tenant_id
)
.fetch_one(&pool)
.await?;

// ❌ Bad - no compile-time checking
let task: Task = sqlx::query("SELECT * FROM tasks WHERE id = $1")
    .bind(task_id)
    .fetch_one(&pool)
    .await?;
```

#### Enforce Tenant Isolation

**Every query must filter by `tenant_id`** (unless it's a system-level query):

```rust
// ✅ Good
SELECT * FROM tasks WHERE id = $1 AND tenant_id = $2

// ❌ Bad - missing tenant isolation
SELECT * FROM tasks WHERE id = $1
```

#### Use Transactions

**Use transactions for multi-step operations**:

```rust
let mut tx = pool.begin().await?;

sqlx::query!("INSERT INTO tasks ...")
    .execute(&mut *tx)
    .await?;

sqlx::query!("INSERT INTO task_events ...")
    .execute(&mut *tx)
    .await?;

tx.commit().await?;
```

### Security Standards

#### Never Log Secrets

**Redact sensitive data in logs**:

```rust
// ✅ Good
tracing::info!(user_id = %user.id, "User logged in");

// ❌ Bad
tracing::info!(api_key = %api_key, "API key used");
```

#### Hash API Keys

**Store only hashed API keys**:

```rust
// ✅ Good
let hash = hash_api_key(&key);
sqlx::query!("INSERT INTO api_keys (hash) VALUES ($1)", hash)
    .execute(&pool)
    .await?;

// ❌ Bad
sqlx::query!("INSERT INTO api_keys (key) VALUES ($1)", key)
    .execute(&pool)
    .await?;
```

#### Validate All Input

**Use validation libraries** (e.g., `validator` crate):

```rust
use validator::Validate;

#[derive(Validate)]
pub struct CreateTaskRequest {
    #[validate(length(min = 1, max = 255))]
    pub name: String,
    #[validate(email)]
    pub notify_email: Option<String>,
}

let req = CreateTaskRequest { ... };
req.validate()?;
```

---

## Testing Requirements

### Coverage Target

**Minimum 80% code coverage** for all new code. CI will enforce this.

### Test Types

#### Unit Tests

**Test all business logic**:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_task_name() {
        assert!(validate_name("valid-name"));
        assert!(!validate_name(""));
        assert!(!validate_name(&"a".repeat(300)));
    }
}
```

#### Integration Tests

**Test all API endpoints and database interactions**:

```rust
// tests/integration/api.rs
#[tokio::test]
async fn test_create_task_endpoint() {
    let app = test_app().await;

    let response = app
        .post("/mcp/start_task")
        .json(&json!({
            "name": "test-task",
            "adapter": "mock",
            "args": {}
        }))
        .send()
        .await;

    assert_eq!(response.status(), StatusCode::CREATED);
    let body: TaskResponse = response.json().await;
    assert_eq!(body.name, "test-task");
}
```

#### Property-Based Tests (Optional)

Use `proptest` for complex logic:

```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn test_hash_chain_integrity(events in prop::collection::vec(any::<Event>(), 1..100)) {
        let chain = build_hash_chain(&events);
        assert!(verify_hash_chain(&chain));
    }
}
```

### Test Organization

```
crate-name/
├── src/
│   └── lib.rs         # Code with inline unit tests (#[cfg(test)])
└── tests/
    ├── integration/   # Integration tests
    │   ├── api.rs
    │   └── worker.rs
    └── common/        # Shared test utilities
        └── mod.rs
```

### Testing Standards

- **Test happy paths and error paths**
- **Test edge cases** (empty input, max limits, boundary conditions)
- **Test concurrency** if relevant (use `tokio::test` with `flavor = "multi_thread"`)
- **Use descriptive test names**: `test_<what>_<condition>_<expected>`
  - Example: `test_create_task_with_empty_name_returns_error`
- **One assertion per test** when possible (easier to debug)
- **Clean up test data** (use transactions or test databases)

### Running Tests

```bash
# All tests
cargo test

# Specific crate
cargo test -p axontask-api

# Specific test
cargo test test_create_task -- --exact --nocapture

# With coverage
cargo tarpaulin --out Html

# Integration tests only
cargo test --test integration
```

---

## Documentation Standards

### Code Documentation

#### Public Items

**All public items must have `///` doc comments**:

```rust
/// Creates a new task and enqueues it for execution.
///
/// # Arguments
///
/// * `name` - The task name (1-255 characters)
/// * `adapter` - The adapter to use (e.g., "shell", "docker")
/// * `args` - Adapter-specific arguments
///
/// # Returns
///
/// Returns the created task with its ID and metadata.
///
/// # Errors
///
/// Returns `CreateTaskError` if:
/// - Name is empty or too long
/// - Adapter is unknown
/// - Database insertion fails
///
/// # Example
///
/// ```
/// let task = create_task("deploy", "fly", json!({"app": "myapp"})).await?;
/// assert_eq!(task.name, "deploy");
/// ```
pub async fn create_task(
    name: &str,
    adapter: &str,
    args: Value,
) -> Result<Task, CreateTaskError> {
    // ...
}
```

#### Complex Logic

**Add explanatory comments for non-obvious code**:

```rust
// Calculate the hash chain by including the previous hash.
// This ensures tamper-evidence: any modification to a previous
// event will break the hash chain for all subsequent events.
let hash_curr = {
    let mut hasher = Sha256::new();
    hasher.update(&hash_prev);
    hasher.update(&event.data);
    hasher.finalize().to_vec()
};
```

### Documentation Files

#### When to Update

Update documentation when:
- Adding a new feature
- Changing an API
- Modifying configuration options
- Adding new patterns or best practices
- Discovering common issues (add to troubleshooting)

#### What to Update

- `CLAUDE.md`: Architecture changes, new patterns, commands
- `README.md`: High-level changes (features, quick start)
- `docs/api/`: API endpoint changes
- `docs/self-hosting/`: Configuration or deployment changes
- `ROADMAP.md`: Mark tasks complete, update status

### API Documentation

**Document all endpoints in `docs/api/`**:

```markdown
## POST /mcp/start_task

Start a new background task.

### Request

```json
{
  "name": "deploy-app",
  "adapter": "fly",
  "args": { "app": "myapp" },
  "timeout_s": 900
}
```

### Response (201 Created)

```json
{
  "task_id": "uuid",
  "stream_url": "/mcp/tasks/{id}/stream",
  "resume_token": "token"
}
```

### Errors

- `400`: Invalid input
- `401`: Unauthorized
- `429`: Rate limit exceeded
```

---

## Commit Guidelines

### Commit Message Format

We follow the [Conventional Commits](https://www.conventionalcommits.org/) specification:

```
<type>(<scope>): <subject>

<body>

<footer>
```

#### Type

- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation changes
- `test`: Adding or updating tests
- `refactor`: Code refactoring (no functional changes)
- `perf`: Performance improvements
- `chore`: Maintenance (dependencies, tooling, etc.)
- `ci`: CI/CD changes

#### Scope (Optional)

The scope should be the crate or component affected:
- `api`: Changes to axontask-api
- `worker`: Changes to axontask-worker
- `shared`: Changes to axontask-shared
- `db`: Database migrations or schema
- `docs`: Documentation
- `ci`: CI/CD

#### Subject

- Use imperative mood ("add" not "added")
- Don't capitalize first letter
- No period at the end
- Max 72 characters

#### Body (Optional)

- Explain **why** the change is needed (not what changed)
- Wrap at 72 characters

#### Footer (Optional)

- Reference issues: `Closes #123`
- Breaking changes: `BREAKING CHANGE: <description>`

### Examples

```
feat(api): add resume_task endpoint

Implement the resume_task endpoint to allow clients to reconnect
to task streams after disconnection. Supports backfill from
arbitrary sequence numbers.

Closes #45
```

```
fix(worker): prevent race condition in heartbeat

The heartbeat goroutine could race with task completion, causing
phantom heartbeats for completed tasks. Now we check task state
before sending each heartbeat.
```

```
docs: update self-hosting guide with SSL setup

Add section on configuring SSL/TLS with Let's Encrypt for
production deployments.
```

```
test(api): add integration tests for rate limiting

Add tests for per-tenant, per-key, and per-route rate limits.
Includes burst handling and refill scenarios.
```

### Commit Hygiene

- **One logical change per commit**: Don't mix unrelated changes
- **Commit often**: Small, focused commits are easier to review
- **Write good commit messages**: Help reviewers understand your changes
- **Test before committing**: Run `cargo test` and `cargo clippy`

---

## Pull Request Process

### Before Opening a PR

**Ensure your code is ready**:

1. ✅ All tests pass: `cargo test`
2. ✅ Code is formatted: `cargo fmt --all`
3. ✅ Linter passes: `cargo clippy --all-targets --all-features -- -D warnings`
4. ✅ Documentation is updated
5. ✅ ROADMAP.md is updated (if applicable)

### Opening a PR

1. **Push your branch** to your fork
2. **Open a pull request** on GitHub
3. **Fill out the PR template** (if available)
4. **Link related issues**: Use `Closes #123` in the description

### PR Title

Follow the same format as [Commit Guidelines](#commit-guidelines):

```
feat(api): add resume_task endpoint
```

### PR Description

**Use this template**:

```markdown
## Summary
Brief description of the changes.

## Motivation
Why is this change needed? What problem does it solve?

## Changes
- Bullet list of specific changes
- Include any breaking changes
- Note any new dependencies

## Testing
How was this tested?
- [ ] Unit tests added/updated
- [ ] Integration tests added/updated
- [ ] Manual testing performed

## Checklist
- [ ] Tests pass locally
- [ ] Code is formatted (cargo fmt)
- [ ] Linter passes (cargo clippy)
- [ ] Documentation updated
- [ ] ROADMAP.md updated (if applicable)

## Related Issues
Closes #123
```

---

## Review Process

### For PR Authors

- **Respond to feedback**: Address all review comments
- **Be receptive**: Reviews improve code quality
- **Explain your choices**: If you disagree with feedback, explain why
- **Keep PRs small**: Easier to review (aim for <500 lines changed)

### For Reviewers

- **Be respectful**: Phrase feedback constructively
- **Be specific**: Provide concrete suggestions
- **Ask questions**: If something is unclear, ask rather than assume
- **Approve when ready**: Don't hold up PRs unnecessarily

### Review Checklist

- [ ] Code follows style guidelines
- [ ] Tests are comprehensive and pass
- [ ] Documentation is complete and accurate
- [ ] No security issues (SQL injection, XSS, etc.)
- [ ] No performance regressions
- [ ] Error handling is proper
- [ ] Tenant isolation is enforced (if applicable)
- [ ] Secrets are not logged

### Merge Requirements

**All PRs must meet these requirements before merging**:

1. ✅ All CI checks pass
2. ✅ At least one approval from a maintainer
3. ✅ All review comments addressed
4. ✅ No merge conflicts
5. ✅ Branch is up-to-date with main

---

## Questions?

If you have questions about contributing:

1. **Check the documentation**: `docs/` directory and CLAUDE.md
2. **Search existing issues**: Your question may already be answered
3. **Open a discussion**: GitHub Discussions for questions
4. **Join the community**: (Add community links if available)

---

## License

By contributing to AxonTask, you agree that your contributions will be licensed under the same license as the project (see LICENSE file).

---

**Thank you for contributing to AxonTask!** 🚀
