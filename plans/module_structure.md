# Plan: Reorganize `loco-laravel` into a module-based architecture (like `goravel`)

## Context

The project currently follows classic Rails-style MVC: code is split by **layer**
(`src/controllers/`, `src/models/`, `src/views/`, `src/mailers/`, `src/workers/`,
`src/tasks/`), so one feature ("auth") is scattered across six directories. The
user has a separate Go project, `goravel` (a Laravel-style framework app), that
they already organized by **feature module** instead
(`app/modules/<name>/{controllers,models,requests,services}/` + a
self-registering `Module` interface + a central registry), and wants the same
philosophy applied here.

An Explore agent fully investigated `goravel`'s convention (directory anatomy,
the `Module` interface, the central registry, cross-module boundaries, testing,
and its `docs/modules.md`) and separately I inspected Loco's generator internals
(`loco-gen` crate: `src/model.rs`, `src/templates/{model,controller,task,worker,
mailer,scaffold}/*.t`) to find out exactly which generators would conflict with
moving files into per-module folders. Key findings that shape this plan:

- **Migrations and entities need zero changes.** `cargo loco generate model`
  only renders a migration file (`migration/src/<ts>_<name>.rs`, already a
  flat/shared crate — this *already* matches goravel's own choice to keep
  `database/migrations/` centralized) and then regenerates
  `src/models/_entities/**` wholesale via `cargo loco db entities`. The
  hand-written model-logic file (e.g. today's `src/models/users.rs`, with
  `ActiveModelBehavior`, `create_with_password`, JWT helpers, etc.) is **never
  generated** — it's 100% hand-authored already, so it can live anywhere from
  day one with no generator conflict.
- **Controllers, tasks, workers, mailers, and `scaffold` *do* hard-code flat
  destinations** in their `.t` templates (`to: "src/controllers/{{name}}.rs"`,
  `to: "src/tasks/{{file_name}}.rs"`, `to: "src/workers/{{module_name}}.rs"`,
  `to: "src/mailers/{{module_name}}.rs"` + templates, `to:
  src/dtos/{{snake_plural}}.rs`) and auto-inject `pub mod` / route / task /
  worker registration lines into `src/{controllers,tasks,workers,mailers}/
  mod.rs` and `src/app.rs`. These generators cannot be reconfigured. Adopting
  the module layout means: after running one of these five generators, you
  manually move the new file(s) into the right `src/modules/<name>/` file and
  relocate the injected line from `src/app.rs`/`mod.rs` into the module's own
  `routes()`/`register_tasks`/`connect_workers` composition point. This mirrors
  goravel's own reality — its `docs/modules.md` "Create A New Module" section
  is itself a 13-step **manual** checklist; goravel has no auto-placing
  generator either. The user explicitly chose this "full physical reorg" cost
  over a no-relocation "registry-only" alternative.
- Goravel keeps a **shared kernel** outside `app/modules/` for genuinely
  cross-cutting code (`app/facades/`, `app/core/`, `app/http/middleware/`) and
  keeps tests **flat and centralized** (`tests/<feature>_test.go`), not
  colocated inside modules. Both map directly onto things this project already
  does correctly (`src/initializers/`, `src/dtos/`, `src/data/`,
  `src/workers/downloader.rs`, and the existing `tests/{models,requests,tasks,
  views,workers}/` layout) — no change needed there.
- Goravel's `Module` interface also carries `Dependencies()` (for a validated
  enable/disable dependency graph) and `Seeders()`. With exactly **one** real
  feature module today (`auth`) and no requested enable/disable behavior, that
  machinery would be speculative complexity with nothing to validate yet. This
  plan intentionally **omits** it (see "Deferred / out of scope").

## Target layout

```
src/
├── app.rs                    # Hooks impl — now calls modules::register_routes(...)
├── lib.rs                    # pub mod modules; replaces pub mod {mailers,tasks,views}
├── controllers/
│   ├── mod.rs                #  pub mod home;  (auth entry removed)
│   └── home.rs                # unchanged — shared/app-level, not a feature module
├── models/
│   ├── mod.rs                # pub mod _entities;  (users.rs entry removed)
│   └── _entities/              # UNTOUCHED — sea-orm-codegen owned, stays flat/shared
├── modules/                    # NEW — one folder per feature
│   ├── mod.rs                  # Module trait + registry (see below)
│   └── auth/
│       ├── mod.rs               # pub mod {controllers,models,requests,views,mailer,tasks,routes}; + AuthModule
│       ├── routes.rs             # pub fn routes() -> Routes  (moved from bottom of controllers/auth.rs)
│       ├── controllers.rs        # moved from src/controllers/auth.rs (handlers only)
│       ├── requests.rs           # NEW home for all param/request structs (see below)
│       ├── views.rs              # moved from src/views/auth.rs (LoginResponse, CurrentResponse)
│       ├── tasks.rs              # moved from src/tasks/user_create.rs (UserCreate)
│       ├── mailer.rs             # moved from src/mailers/auth.rs, include_dir! paths updated
│       ├── mailer/
│       │   ├── welcome/ forgot/ magic_link/   # moved from src/mailers/auth/*
│       └── models/
│           ├── mod.rs            # pub mod users;
│           └── users.rs          # moved from src/models/users.rs, entity import path fixed
├── dtos/, data/, initializers/, workers/   # UNCHANGED — shared/app-level, same reasoning as goravel's app/core + app/facades
migration/, tests/, assets/, config/         # UNCHANGED (see "What does not move" below)
```

## What moves, and exact old → new mapping

| Today | Moves to | Notes |
|---|---|---|
| `src/controllers/auth.rs` (handlers) | `src/modules/auth/controllers.rs` | keep handler bodies as-is |
| `src/controllers/auth.rs` (`routes()` fn) | `src/modules/auth/routes.rs` | separate file, mirrors goravel's `routes.go` |
| `src/controllers/auth.rs` (`ForgotParams`, `ResetParams`, `MagicLinkParams`, `ResendVerificationParams`) | `src/modules/auth/requests.rs` | |
| `src/models/users.rs` (`LoginParams`, `RegisterParams`) | `src/modules/auth/requests.rs` | consolidates all request/input DTOs in one place, matching goravel's `requests/` bucket |
| `src/models/users.rs` (rest: `Validator`, `ActiveModelBehavior` impl, business-logic methods) | `src/modules/auth/models/users.rs` | keep `Validator` here — it's tied to `before_save`, not an external request shape |
| `src/views/auth.rs` | `src/modules/auth/views.rs` | unchanged content |
| `src/mailers/auth.rs` + `src/mailers/auth/{welcome,forgot,magic_link}/*.t` | `src/modules/auth/mailer.rs` + `src/modules/auth/mailer/{welcome,forgot,magic_link}/*.t` | **must** update the three `include_dir!("src/mailers/auth/...")` string literals to `"src/modules/auth/mailer/..."` — these are crate-root-relative string literals, not auto-relative to the source file |
| `src/tasks/user_create.rs` | `src/modules/auth/tasks.rs` | |

## What does NOT move (and why)

- `src/models/_entities/**` — sea-orm-codegen generated, `cargo loco db entities`
  always rewrites this exact path. Equivalent to goravel's centralized
  migrations.
- `migration/**` — already flat/shared; matches goravel's own centralized
  `database/migrations/`. `cargo loco generate migration` needs no changes.
- `src/fixtures/users.yaml` — path is passed in by Loco's own seed-runner
  convention (`Hooks::seed(ctx, base)`), same "tool-fixed location" reasoning
  as entities.
- `assets/views/home/*`, `assets/i18n/**` — belong to `home`, not a feature
  module (see below).
- `src/controllers/home.rs` — stays put. It's the app's landing/demo page, not
  a business feature (no model, no requests) — goravel itself keeps this kind
  of shared/static thing outside `app/modules/`.
- `src/workers/downloader.rs` — generic scaffold example, not tied to the auth
  domain; stays as shared/example infra (goravel's own `app/core/` equivalent).
- `src/dtos/`, `src/data/`, `src/initializers/` — already-shared/cross-cutting,
  unchanged.
- `tests/**` — Loco's integration tests already live in one flat top-level
  crate (required by the `request::<App,_,_>` test harness) organized by
  *test-type* (`models/`, `requests/`, `tasks/`, `views/`, `workers/`) — this
  **already matches** goravel's own choice to keep `tests/` flat and NOT
  colocate tests inside modules. No structural change, only import-path fixes
  (see below).

## The `Module` trait + registry (new, minimal)

Deliberately smaller than goravel's `Module` interface — no `Dependencies()`/
`Seeders()`/enable-disable, since nothing needs them yet (see "Deferred"):

```rust
// src/modules/mod.rs
pub mod auth;

use loco_rs::controller::{AppRoutes, Routes};

pub trait Module {
    /// Bookkeeping/logging only today.
    fn name(&self) -> &'static str;
    fn routes(&self) -> Routes;
}

fn registry() -> Vec<Box<dyn Module>> {
    vec![Box::new(auth::AuthModule)]
}

/// Fold every registered module's routes into `AppRoutes`, replacing the
/// hand-maintained `.add_route(controllers::auth::routes())` chain.
pub fn register_routes(mut routes: AppRoutes) -> AppRoutes {
    for module in registry() {
        routes = routes.add_route(module.routes());
    }
    routes
}
```

```rust
// src/modules/auth/mod.rs
pub mod controllers;
pub mod mailer;
pub mod models;
pub mod requests;
pub mod routes;
pub mod tasks;
pub mod views;

pub struct AuthModule;

impl super::Module for AuthModule {
    fn name(&self) -> &'static str { "auth" }
    fn routes(&self) -> loco_rs::controller::Routes { routes::routes() }
}
```

## Wiring changes

- **`src/lib.rs`**: replace `pub mod mailers; pub mod tasks; pub mod views;`
  with `pub mod modules;` (controllers/, models/, data/, dtos/, initializers/,
  workers/ stay).
- **`src/controllers/mod.rs`**: drop `pub mod auth;`, keep `pub mod home;`.
- **`src/models/mod.rs`**: drop `pub mod users;`, keep `pub mod _entities;`.
- **`src/app.rs`**:
  - `use crate::{controllers, initializers, models::_entities::users, modules, tasks, workers::downloader::DownloadWorker};` → drop the now-gone `tasks` import, add `modules`, keep `models::_entities::users` (still used by `truncate`/`seed`).
  - `routes()`: `AppRoutes::with_default_routes().add_route(controllers::home::routes())` piped through `modules::register_routes(...)` instead of the old `.add_route(controllers::auth::routes())` line.
  - `register_tasks()`: `tasks.register(tasks::user_create::UserCreate)` → `tasks.register(modules::auth::tasks::UserCreate)`.
- **`src/modules/auth/models/users.rs`**: change `pub use super::_entities::users::{...}` (currently relative to `crate::models`) to the absolute path `pub use crate::models::_entities::users::{self, ActiveModel, Entity, Model};`.
- **`src/modules/auth/mailer.rs`**: update the three `include_dir!(...)` literals as noted above.
- **`src/modules/auth/controllers.rs`**: update its `use` block — `mailers::auth::AuthMailer` → `super::mailer::AuthMailer`, `models::{_entities::users, users::{LoginParams, RegisterParams}}` → `crate::models::_entities::users` + `super::requests::{LoginParams, RegisterParams}`, `views::auth::{...}` → `super::views::{...}`.
- **`src/modules/auth/tasks.rs`**: same kind of `use` fixup (`mailers::auth::AuthMailer` → `super::mailer::AuthMailer`, `models::{_entities::users, users::RegisterParams}` → `crate::models::_entities::users` + `super::requests::RegisterParams`).

## Test-file import fixes (content unchanged, only `use` paths)

Exhaustive list from grepping `models::users|views::auth|mailers::auth|tasks::user_create` across `tests/`:

- `tests/requests/prepare_data.rs`: `loco_laravel::{models::users, views::auth::LoginResponse}` → `loco_laravel::modules::auth::{models::users, views::LoginResponse}`
- `tests/requests/auth.rs`: `loco_laravel::{app::App, models::users}` → `loco_laravel::{app::App, modules::auth::models::users}`
- `tests/tasks/user_create.rs`: same fix as above
- `tests/models/users.rs`: `models::users::{self, Model, RegisterParams}` → `modules::auth::models::users::{self, Model}` + `modules::auth::requests::RegisterParams` (since `RegisterParams` is moving out of `models::users` into `requests`)
- `tests/requests/home.rs`, `src/bin/main.rs`: only reference `loco_laravel::app::App` — unaffected, no change needed.

## Documentation updates (required, not optional)

1. **`AGENTS.md`** — rewrite the "Where things live" tree to show `src/modules/<feature>/` as the primary home for feature code, keep the shared/flat entries (`_entities/`, `migration/`, `controllers/home.rs`, `workers/`, `dtos/`, `initializers/`) with a one-line note on why they stay flat, and add a short paragraph on the generator caveat (which 5 generators need a manual post-generate move, which 2 don't).
2. **New `docs/modules.md`** (mirrors goravel's own authoritative doc) — the module skeleton, the `Module` trait, the registry, and a concrete "Create a New Module" checklist including the exact manual relocation step per generator (`controller`, `task`, `worker`, `mailer`, `scaffold` need it; `model`/`migration` don't).

## Deferred / explicitly out of scope

- Goravel-style `Dependencies()` + dependency-graph validation + `config/modules.json` enable/disable — no second module exists yet to depend on anything, and no one asked for runtime module toggling. Add this only when a real cross-module dependency shows up.
- A `services/` layer — goravel's `catalog` module optionally adds one, but this project's Loco convention already keeps business logic on the model (ActiveModel/Model methods), matching goravel's `auth`/`blog` modules (which access the ORM directly from `services/`, i.e. a thin pass-through) — introducing a new layer here would be unrequested abstraction.
- A `repositories/` layer (goravel's `catalog` only) — same reasoning, skip unless a future module actually needs swappable persistence.

## Verification (after implementation)

1. `cargo check --all-features` — confirms every moved `use`/path compiles.
2. `cargo test --all-features --all` — expect the same 28 tests passing (no behavior change, only paths); this exercises the mailer's `include_dir!` paths, the model's entity re-export, and the moved task/controller wiring end-to-end.
3. `cargo fmt --all -- --check` and `cargo clippy --all-features -- -D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms` — match CI (`.github/workflows/ci.yaml`).
4. `cargo loco routes` — confirm `/api/auth/*` and `/`, `/hello` routes are all still registered identically to before.
5. Manually smoke-test one generator relocation end-to-end (e.g. `cargo loco generate task ping`) to validate the documented "generate then relocate" checklist actually works as written.
