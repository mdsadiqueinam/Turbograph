# Contributing to Turbograph

Thank you for your interest in contributing! This document describes the recommended workflow and conventions to keep contributions consistent and reviewable.

## Table of Contents

- [Getting started](#getting-started)
- [Development environment](#development-environment)
- [Running tests](#running-tests)
- [Code style](#code-style)
- [Documentation](#documentation)
- [Submitting changes](#submitting-changes)
- [Reporting issues](#reporting-issues)

---

## Getting started

1. Fork the repository and clone your fork.
2. Create a feature branch from `main`:

   ```bash
   git checkout -b my-feature
   ```

3. Make your changes and commit them with a descriptive message.
4. Push the branch and open a pull request against `main`.

---

## Development environment

**Prerequisites**

| Tool | Minimum version |
|------|-----------------|
| Rust | 1.80 (Edition 2021) |
| Docker (for tests) | any recent version |
| PostgreSQL | 14+ (via Docker) |

**Start the database**

```bash
docker compose up -d postgres
```

The compose file starts a PostgreSQL instance pre-loaded with the schema in `db/init.sql`.

**Build the project**

```bash
cargo build
```

**Run the example server**

```bash
cargo run --manifest-path examples/server/Cargo.toml
# Open http://localhost:4000/graphql in a browser
```

---

## Running tests

Unit tests (no database required):

```bash
cargo test --package turbograph --lib
```

All tests, including the integration test (requires PostgreSQL running):

```bash
cargo test --package turbograph --lib --tests
```

Run a single test by name:

```bash
cargo test --package turbograph <test_name> -- --nocapture
```

---

## Code style

- Format code with `cargo fmt` before committing.
- Fix all `cargo clippy` warnings before submitting a PR:

  ```bash
  cargo clippy -- -D warnings
  ```

- Prefer `///` doc comments on every public item.
- `unsafe` blocks must have a `// SAFETY:` comment that explains why the code is sound.
- Do not add `.unwrap()` in library code — propagate errors with `?` or return an `Option`.

---

## Documentation

Every public struct, enum, trait, method, and function must have a `///` doc comment.
Aim for:

- A short one-line summary.
- A longer description if behaviour is non-obvious.
- A `# Errors` section when a function returns `Result`.
- A `# Example` section with a runnable or `no_run` snippet.

Internal helpers (private or `pub(crate)`) should have at minimum a brief comment explaining what they do.

To preview the generated documentation locally:

```bash
cargo doc --no-deps --open
```

---

## Submitting changes

- Keep pull requests focused on a single concern.
- Include tests for new behaviour whenever feasible.
- Ensure `cargo test --lib` passes before opening a PR.
- Write a clear PR description explaining *what* and *why*.

---

## Reporting issues

Please use the [GitHub issue tracker](https://github.com/mdsadiqueinam/Turbograph/issues) to report bugs or request features. When filing a bug, include:

- Turbograph version (crate version or git commit).
- PostgreSQL version.
- A minimal reproduction (SQL schema + GraphQL query).
- The observed output and the expected output.
