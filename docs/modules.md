# Module Structure

This project organizes feature code by **module** — one folder per feature
under `src/modules/<name>/` — instead of Loco's default layer-first layout
(`src/controllers/`, `src/models/`, `src/views/`, ...). This mirrors the
convention used in this team's `goravel` project: each feature owns its
controllers, models, request/DTO structs, views, mailer, and tasks in one
place, and self-registers into the app through a small `Module` trait +
central registry instead of being hand-wired line-by-line into `src/app.rs`.

## Anatomy of a module

```
src/modules/<name>/
  mod.rs          # pub mod {controllers,models,requests,views,mailer,tasks,routes}; + a `<Name>Module` implementing `Module`
  routes.rs       # pub fn routes() -> Routes
  controllers.rs  # HTTP handlers (pub(super), called only from routes.rs)
  requests.rs     # request/param structs (the Loco equivalent of "DTOs")
  views.rs        # response structs returned by controllers
  mailer.rs       # Mailer impl; include_dir!() paths point at this module's mailer/ folder
  mailer/         # per-mail template folders (html.t/text.t/subject.t)
  tasks.rs        # CLI tasks (`cargo loco task ...`) owned by this feature
  models/
    mod.rs
    <name>.rs     # hand-written ActiveModelBehavior/business logic — NOT the generated entity
```

Not every module needs every piece — a module with no email need not have
`mailer.rs`. See `src/modules/auth/` for a complete worked example.

## What stays shared/flat, and why

- **`src/models/_entities/`** — Sea-ORM-codegen generated. `cargo loco db
  entities` always rewrites this exact path from the DB schema; never move or
  hand-edit it. Every module's `models/<name>.rs` imports from here via
  `crate::models::_entities::<name>`.
- **`migration/`** — Sea-ORM migrations stay in one flat crate/folder.
  `cargo loco generate migration` targets this unconditionally; there's no
  per-module migrations folder, same as this project's `goravel` reference
  keeps `database/migrations/` centralized. Ownership is by filename/domain
  convention only.
- **`src/fixtures/`** — seed data path is fixed by Loco's own
  `Hooks::seed(ctx, base)` convention, same reasoning as entities.
- **`src/controllers/home.rs`**, **`assets/views/home/**`**, **`assets/i18n/**`**
  — the landing/demo page isn't a business feature (no model, no requests), so
  it stays outside `src/modules/`.
- **`src/workers/`** — background jobs not yet tied to a specific feature
  (e.g. the generic `downloader.rs` example) stay here until they belong to a
  module.
- **`src/dtos/`, `src/data/`, `src/initializers/`** — genuinely cross-cutting
  app infrastructure (the typed API/frontend contract layer, the Tera/i18n
  view-engine initializer), shared by every module.
- **`tests/`** — stays flat, organized by test-type
  (`models/`, `requests/`, `tasks/`, `views/`, `workers/`), matching Loco's own
  integration-test harness (`request::<App, _, _>`) and this project's
  `goravel` reference, which also keeps tests centralized rather than
  colocated inside modules.
- **`src/tasks/mod.rs`** and **`src/mailers/mod.rs`** — kept as
  intentionally-empty files. They aren't wired into `src/lib.rs`'s module
  tree beyond existing on disk; see "Generator caveat" below for why they
  must exist.

## The `Module` trait + registry

`src/modules/mod.rs`:

```rust
pub trait Module {
    fn name(&self) -> &'static str;   // bookkeeping/logging only today
    fn routes(&self) -> loco_rs::controller::Routes;
}
```

Each module implements it once in its own `mod.rs` (see
`src/modules/auth/mod.rs`) and is added to the `registry()` function in
`src/modules/mod.rs`. `src/app.rs`'s `Hooks::routes()` calls
`modules::register_routes(...)` once instead of one `.add_route(...)` per
feature.

This is deliberately smaller than `goravel`'s `Module` interface — no
`Dependencies()`, no `Seeders()`, no enable/disable config. Add those only
when a second module actually needs to depend on another one or you need
runtime module toggling; with one module today they'd be speculative
complexity.

## Generator caveat (read before running a generator)

`loco-gen`'s templates hard-code their output paths — this cannot be
reconfigured. Two generators need **zero changes to your workflow**:

- `cargo loco generate model <name> <fields>` and `cargo loco generate
  migration <name>` only touch `migration/` and `src/models/_entities/`,
  which are supposed to stay flat/shared anyway. Write the hand-authored
  model-logic file directly at `src/modules/<name>/models/<name>.rs` — Loco
  never generates that file for you, so there's nothing to relocate.

The rest land in flat legacy locations and self-inject into `src/app.rs` /
a flat `mod.rs`. After running one of these, relocate by hand:

| Generator | Lands at | Manual relocation |
|---|---|---|
| `cargo loco generate controller <name>` | `src/controllers/<name>.rs`, injects `src/controllers/mod.rs` + a route line in `src/app.rs` | Move the file's content into `src/modules/<name>/controllers.rs` (+ `routes.rs` for the route line), remove the `pub mod` line from `src/controllers/mod.rs`, remove the injected line from `src/app.rs`, delete the leftover file |
| `cargo loco generate task <name>` | `src/tasks/<name>.rs`, injects `src/tasks/mod.rs` (the empty landing zone) + `register_tasks` in `src/app.rs` | Move into `src/modules/<name>/tasks.rs`, remove the injected `pub mod` line from `src/tasks/mod.rs`, move the `tasks.register(...)` line into the module, delete the leftover file |
| `cargo loco generate worker <name>` | `src/workers/<name>.rs`, injects `src/workers/mod.rs` + `connect_workers` in `src/app.rs` | Same pattern: move into `src/modules/<name>/workers.rs`, fix the two injections |
| `cargo loco generate mailer <name>` | `src/mailers/<name>.rs` + `src/mailers/<name>/welcome/*.t`, injects `src/mailers/mod.rs` (the empty landing zone) | Move the `.rs` file and its template folder into `src/modules/<name>/mailer.rs` + `src/modules/<name>/mailer/`, **update the `include_dir!(...)` string literals** (they're crate-root-relative, not relative to the source file), remove the injected line, delete the leftovers |
| `cargo loco generate scaffold <name> ...` | `src/dtos/<plural>.rs` + `src/controllers/<plural>.rs`, injects both `mod.rs` files + a route line | Same as `controller`, plus decide whether the DTO belongs in the module's `requests.rs`/`views.rs` or genuinely belongs in the shared `src/dtos/` |

`src/tasks/mod.rs` and `src/mailers/mod.rs` must keep existing (even though
they're empty and not referenced by real code) — `cargo loco generate
task|mailer` fails outright with `InjectionTargetMissing` if that file is
gone. Do not delete them.

## Create a new module — checklist

1. `mkdir -p src/modules/<name>/models`
2. Write `src/modules/<name>/models/<name>.rs` by hand (entity import from
   `crate::models::_entities::<name>`, business-logic methods, `Validator`).
   Generate the migration/entity first with `cargo loco generate model <name>
   <fields>` — no relocation needed for that part.
3. Write `requests.rs`, `views.rs`, `controllers.rs`, `routes.rs` following
   `src/modules/auth/` as the template.
4. Add `pub struct <Name>Module;` implementing `Module` in the module's
   `mod.rs`, and register it in `src/modules/mod.rs`'s `registry()`.
5. If you used `cargo loco generate task|worker|mailer|controller|scaffold`
   along the way, apply the relocation from the table above.
6. Add tests under `tests/{models,requests,tasks,views,workers}/` (flat, not
   inside the module — see "What stays shared/flat").
7. Run `cargo check --all-features`, `cargo test --all-features --all`,
   `cargo fmt --all -- --check`, `cargo clippy --all-features -- -D warnings
   -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms`, and `cargo
   loco routes` to confirm the new routes are registered.
