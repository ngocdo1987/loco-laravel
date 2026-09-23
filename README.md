# loco-laravel

A [Loco](https://loco.rs) (Rust web framework, Laravel/Rails-style) **SaaS
starter**: JWT-based authentication (register, email verification, login,
forgot/reset password, magic-link login) on top of Sea-ORM, with a
Redis-backed background worker, server-side rendering via Tera + Fluent
i18n, and a typed API/DTO layer (`ts-rs`) for a future SPA frontend.

Feature code is organized **by module** (one folder per feature under
`src/modules/<name>/`), not by Loco's default layer-first layout — see
[`docs/modules.md`](docs/modules.md) for the convention and
[`AGENTS.md`](AGENTS.md) for the full agent-facing guide to this codebase.

## Stack

- **[Loco](https://loco.rs)** — routing, background jobs, tasks, mailers,
  the scheduler, and testing harness
- **Sea-ORM** — Postgres in production/CI, SQLite by default for local dev
- **Redis** — background job queue (`worker_redis`)
- **Tera + Fluent** — server-side views with i18n (`assets/i18n/`)
- **ts-rs** — generates TypeScript bindings from `src/dtos/**` for a
  separate frontend project (not included in this repo)

## Getting started

Requirements: Rust (stable), a local Redis instance, and either Postgres or
the SQLite default.

```sh
cargo loco start
```

This boots the server (default `http://localhost:5150`, override with
`PORT=...`) and exposes:

| Route | Description |
|---|---|
| `GET /` | Landing page (Tera) listing the demo route and API endpoints |
| `GET /hello` | Tera + Fluent i18n rendering demo |
| `POST /api/auth/register` | Create an account, sends a welcome/verification email |
| `GET /api/auth/verify/{token}` | Verify an email address |
| `POST /api/auth/login` | Password login, returns a JWT |
| `POST /api/auth/forgot` / `POST /api/auth/reset` | Password reset flow |
| `POST /api/auth/magic-link` / `GET /api/auth/magic-link/{token}` | Passwordless login |
| `GET /api/auth/current` | Current user (requires `Authorization: Bearer <jwt>`) |
| `POST /api/auth/resend-verification-mail` | Re-send the verification email |

Run `cargo loco routes` at any time to list the live route table.

## Configuration

Per-environment YAML lives in `config/{development,test,production}.yaml`
(`LOCO_ENV` selects one). Secrets are read from the environment via the
`get_env` Tera helper — notably `DATABASE_URL`, `REDIS_URL`, and
`JWT_SECRET` (production only; development/test ship throwaway defaults).

## Testing

```sh
cargo test --all-features --all
```

Needs a reachable Postgres and Redis (see `.github/workflows/ci.yaml` for
the exact services CI spins up); locally, the `test`/`development` configs
default to a local SQLite file and `redis://127.0.0.1` if you don't set
`DATABASE_URL`/`REDIS_URL`. Snapshot assertions use `insta`.

## Adding a new feature

New features are added as a module under `src/modules/<name>/`, following
`src/modules/auth/` as the reference implementation. `docs/modules.md` has
the full checklist, including the manual relocation step required after
`cargo loco generate controller|task|worker|mailer|scaffold` (these
generators write to Loco's default flat locations and can't be
reconfigured; `model`/`migration` need no relocation).

## Learn more

- [Loco docs](https://loco.rs/docs) / [quick tour](https://loco.rs/docs/getting-started/tour/)
- [`AGENTS.md`](AGENTS.md) — commands, conventions, and where things live
- [`docs/modules.md`](docs/modules.md) — the module architecture in detail
