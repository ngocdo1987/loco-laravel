# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

A [Loco](https://loco.rs) (Rust web framework, "Rails/Laravel for Rust") **SaaS starter**: JWT-based auth (register/verify/login/forgot/reset/magic-link) built on Sea-ORM + Postgres, with Redis-backed background workers. See `AGENTS.md` for the framework-level conventions (directory layout, generators, `AppContext`, i64 keys, etc.) — read it first, this file only adds what's specific to this repo.

## Commands

```sh
cargo loco start                      # run the app (http://localhost:5150)
cargo loco db migrate                 # apply migrations
cargo loco routes                     # list routes
cargo loco task <name>                # run a task (e.g. `cargo loco task user_create`)
cargo loco doctor                     # check the environment
cargo test --all-features --all       # run the full test suite
cargo test <test_name>                # run a single test (e.g. `cargo test can_register`)
cargo fmt --all -- --check            # CI style check
cargo clippy --all-features -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms  # CI lint
```

Tests need Postgres and Redis (see `.github/workflows/ci.yaml` for the exact services/env used in CI: `DATABASE_URL`, `REDIS_URL`). Snapshot assertions use `insta`; review/update snapshots under `tests/**/snapshots/` when intentionally changing an endpoint's output.

## Architecture notes beyond AGENTS.md

- **DTOs (`src/dtos/`)** are the typed JSON API contract layer, separate from Sea-ORM models. They derive `ts-rs::TS` with `#[ts(export, export_to = "../frontend/src/bindings/")]` so a companion SPA frontend can consume generated TypeScript bindings. `dtos/common.rs` has the shared `Page<T>` (pagination envelope — build via `Page::from_query`, never by hand, so it stays in lockstep with the framework's own `Pager`) and `ApiError`. When adding a new API response type, add a DTO here rather than serializing entities/view structs directly.
- **Views (`src/views/`)** hold response shapes returned by controllers (e.g. `views::auth::{LoginResponse, CurrentResponse}`) — distinct from the `dtos` module; views are controller-facing response builders, dtos are the frontend-facing typed contract.
- **View engine / i18n (`src/initializers/view_engine.rs`)**: registers a Tera view engine as an Axum extension in `after_routes`. If `assets/i18n/` exists, it wires Fluent-based i18n (`FluentLoader`) with a shared FTL resource at `assets/shared.ftl`. That shared file must live *outside* `assets/i18n/` or the locale scan double-registers it and bundle building fails (see comment in that file / loco-rs/loco#1749) — keep new shared `.ftl` resources outside the per-locale directory.
- **Auth model (`src/models/users.rs`)**: password auth, email verification, forgot/reset password, and magic-link login all live on the `users` model/active-model (`create_with_password`, `find_by_email`, `find_by_verification_token`, `find_by_reset_token`, `find_by_magic_token`, `create_magic_link`, `verify_password`, `generate_jwt`, etc.). `before_save` sets `pid` (UUID) and `api_key` on insert and re-runs the `Validator` (name/email) on every save. Controllers in `src/controllers/auth.rs` deliberately return a generic success response (never leak whether an email exists) for forgot/reset/magic-link/resend flows — preserve that behavior when touching these endpoints.
- **Magic-link email allowlist**: `controllers/auth.rs` restricts magic-link requests to a hardcoded domain regex (`EMAIL_DOMAIN_RE`, currently `@example.com`/`@gmail.com`) — this is a starter placeholder, tighten/replace per real deployment requirements rather than assuming it's production-ready as-is.
- **Mailers (`src/mailers/auth.rs` + `src/mailers/auth/{forgot,magic_link,welcome}/`)**: each mail has sibling `html.t`/`text.t`/`subject.t` Tera templates; add new mail types the same way (generator + matching template triplet).
- **Workers**: only `DownloadWorker` (`src/workers/downloader.rs`) is registered, using the `worker_redis` Loco feature (see `Cargo.toml`) — Redis must be reachable for queued jobs.
- **Tasks**: `UserCreate` (`src/tasks/user_create.rs`) is the only registered task; `# tasks-inject (do not remove)` marker in `src/app.rs` is where `cargo loco generate task` inserts new registrations.
