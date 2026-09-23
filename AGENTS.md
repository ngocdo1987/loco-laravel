# Agent guide for this Loco app

This is a **Loco** (loco.rs) application — an all-in-one, batteries-included Rust
web framework. Routing, the database (Sea-ORM), background jobs, a scheduler,
mailers, tasks, storage, caching, and testing are already integrated. **Prefer
Loco's built-ins and generators over adding external crates or wiring
infrastructure by hand.**

## Where things live

This app organizes feature code by **module** (one folder per feature, like
`goravel`/NestJS), not by Loco's default layer-first layout. See
[`docs/modules.md`](docs/modules.md) for the full convention — short version:

```
src/app.rs              # impl Hooks for App — registers routes/workers/tasks (the wiring hub)
src/modules/            # ONE FOLDER PER FEATURE — put new feature code here
  mod.rs                 # Module trait + central registry (self-registers routes)
  auth/                  # e.g. controllers.rs, models/, requests.rs, views.rs, mailer.rs(+templates), tasks.rs, routes.rs, mod.rs
src/controllers/home.rs # shared/app-level routes that aren't a feature module
src/models/_entities/   # GENERATED Sea-ORM entities — do not hand-edit, shared across all modules
migration/              # Sea-ORM migrations — centralized (like a module's code, not its location)
src/workers/            # shared/example background jobs not yet tied to a module
src/dtos/, src/initializers/, src/data/  # shared/cross-cutting, not feature-specific
config/*.yaml           # per-environment config (LOCO_ENV)
tests/                  # request/model/task tests — flat, organized by test-type (not per-module)
```

## How to work in this app

- **Add features as a module**: create `src/modules/<name>/` following the
  `auth` module's shape, implement the `Module` trait in its `mod.rs`, and add
  it to the registry in `src/modules/mod.rs`. See `docs/modules.md` for the
  step-by-step checklist.
- **Generators still help, but land in flat legacy locations** (a `loco-gen`
  limitation, not configurable): `cargo loco generate model|migration` need
  **no relocation** — they only touch `migration/` and `src/models/_entities/`,
  both intentionally shared/flat. `cargo loco generate
  controller|task|worker|mailer|scaffold` land in
  `src/{controllers,tasks,workers,mailers,dtos}/` and self-wire into
  `src/app.rs` — after running one of these, manually move the new file(s)
  into the right `src/modules/<name>/` file and relocate the injected
  `src/app.rs` line into that module's own `routes()`/`register_tasks`
  composition. `docs/modules.md` has the exact steps per generator.
- **Everything uses `AppContext` (`ctx`)**: `ctx.db`, `ctx.config`,
  `ctx.mailer`, `ctx.storage`, `ctx.cache`, `ctx.queue_provider`. Don't create
  your own DB pool, server, or job queue.
- Start every controller/model/worker/task with `use loco_rs::prelude::*;`.
- App code returns `loco_rs::Result<T>` and uses `?`.
- Config is YAML in `config/`; secrets come from the environment via the
  `get_env` Tera helper inside the YAML.
- Primary/foreign keys are `i64` (this is Loco 0.17+).
- Tests: `request::<App, _, _>(|request, ctx| async move { ... }).await;`.

## Useful commands

```
cargo loco start            # run the app
cargo loco db migrate       # apply migrations
cargo loco routes           # list routes
cargo loco task <name>      # run a task
cargo loco doctor           # check the environment
```

## Learn more

- Framework agent guide: https://loco.rs/AGENTS.md
- Full single-file reference: https://loco.rs/llms-full.txt
- Docs: https://loco.rs/docs
