# Plan: Add web (Tera-rendered) auth controllers to `src/modules/auth`

## Context

The `auth` module currently only exposes a JSON API (`/api/auth/*`). The user
wants a browser-facing UI — login/register forms — mirroring the `web/`
controller branch of their `goravel` reference project, which has its own
session-based login/register/logout pages separate from its JSON API.

An Explore agent read goravel's actual `controllers/web/auth_controller.go`,
`routes.go`, `view_auth.go` middleware, and the literal `login.tmpl`/
`register.tmpl` templates. Key facts from that investigation:

- Goravel's web auth is a **server-side session** (a `view_user_id` stored via
  Goravel's own file-backed session store, delivered to the browser as one
  opaque session-ID cookie) — entirely separate machinery from its JWT-based
  API auth. It has its own `StartSession()` middleware, a `ViewAuth`/
  `ViewGuest` middleware pair (redirect to `/login` or `/dashboard`), and CSRF
  protection (`VerifyCsrfToken`, with a hidden `_token` field in every form).
- Exactly 3 web endpoints exist: `GET/POST /login`, `GET/POST /register`,
  `POST /logout` — **no web forgot/reset/profile pages**, only under the API.
- On validation/auth failure, POST handlers **re-render the same template**
  with a flat error string and the submitted values (HTTP 200, no redirect).
  On success: log the session in and `302` redirect to `/dashboard`.
- Templates are self-contained Bootstrap-5-CDN HTML with no shared
  layout/partial (duplicated head/navbar between the two files).

**Decision: reuse the existing JWT auth, don't build a parallel session
system.** Loco has no built-in session/CSRF middleware equivalent to
Goravel's, and this project's `Model`/mailer/business logic is already
100% JWT-based and battle-tested (28 passing tests). Building a whole second
auth mechanism just to mirror Goravel's *implementation detail* would be a
large, unrequested subsystem. Instead: the web login/register handlers call
the exact same `Model::create_with_password` / `Model::find_by_email` /
`verify_password` / `generate_jwt` already used by the JSON API, and persist
the resulting JWT as an **`auth_token` HttpOnly cookie** instead of returning
it as JSON. Confirmed via `loco_rs::auth::jwt::JWT::validate()` (already used
implicitly by the `auth::JWT` API extractor) that decoding that cookie back
into `UserClaims{ pid, .. }` uses code that already exists — no new auth
primitive, just a different transport for the same token. This is what
"reference goravel's UI and do it similarly" is scoped to: the **look and
flow** (form fields, inline error message, redirect-after-success to
`/dashboard`, guest/protected route guards), not a byte-for-byte port of
Goravel's session backend.

**Explicitly out of scope** (flag in the plan, don't build): CSRF protection
(Loco ships no CSRF middleware to hook into) and a general flash-message
subsystem (Goravel's relies on its session store, which we're not building —
register-success logs the user in immediately and lands on `/dashboard`
already authenticated, achieving the same practical outcome without a flash).

## Routes

New root-level (unprefixed) routes, added alongside the existing
`/api/auth/*` group — mirrors goravel's `RegisterWebRoutes` running on the
plain `web` router, not under `/api`:

| Method | Path | Guard | Behavior |
|---|---|---|---|
| GET | `/login` | redirect to `/dashboard` if already logged in | render login form |
| POST | `/login` | same | authenticate; success → set cookie + redirect `/dashboard`; failure → re-render form with `error` + sticky `email` |
| GET | `/register` | redirect to `/dashboard` if already logged in | render register form |
| POST | `/register` | same | create user + send welcome email (reuse existing logic) + log in immediately; duplicate email → specific "Email already registered" error (matches goravel); other failure → generic error; both re-render with sticky `name`/`email` |
| POST | `/logout` | none | clear the cookie, redirect to `/login` |
| GET | `/dashboard` | redirect to `/login` if not logged in | show the current user (name/email/pid) + a logout button |

## Files

**New:**
- `src/modules/auth/web.rs` — the 6 handlers above, plus a small
  `current_web_user(ctx, &jar) -> Option<users::Model>` helper that reads the
  `auth_token` cookie via `axum_extra::extract::cookie::CookieJar`, validates
  it with `loco_rs::auth::jwt::JWT::new(&ctx.config.get_jwt_config()?.secret).validate(token)`,
  and looks up `users::Model::find_by_pid` from the claims — reusing
  `crate::models::_entities::users` and `super::models::users::Model` exactly
  as `controllers.rs`'s `current` handler already does.
- `assets/views/auth/login.html`, `assets/views/auth/register.html`,
  `assets/views/auth/dashboard.html` — plain HTML + inline `<style>`
  (matching this project's own existing `assets/views/home/index.html`
  style, not Bootstrap-CDN), reproducing goravel's field names
  (`email`/`password`/`name`), the flat error-message box, and the
  login↔register cross-links.
- `tests/requests/auth_web.rs` (registered in `tests/requests/mod.rs`):
  GET `/login`/`/register` render 200; POST `/register` creates a user, sets
  a cookie, and redirects to `/dashboard`; GET `/dashboard` without a cookie
  redirects to `/login`; GET `/dashboard` with the cookie from a successful
  login/register shows the user; POST `/login` with wrong password
  re-renders 200 with an error; GET `/login` while already logged in
  redirects to `/dashboard`.

**Modified:**
- `src/modules/mod.rs` — broaden the `Module` trait from `fn routes(&self)
  -> Routes` to `fn routes(&self) -> Vec<Routes>`, since `auth` now
  contributes two differently-prefixed route groups (`/api/auth/*` and the
  root-level web routes). Update `register_routes` to loop over the vec.
- `src/modules/auth/routes.rs` — keep the existing `pub fn routes()`
  (`/api/auth/*`) unchanged; add `pub fn web_routes() -> Routes` (no
  `.prefix(...)`) wiring the 6 new handlers.
- `src/modules/auth/mod.rs` — add `pub mod web;`; update `AuthModule::routes`
  to `vec![routes::routes(), routes::web_routes()]`.
- `Cargo.toml` — add `"cookie"` to `axum-extra`'s `features` (already
  transitively enabled via `loco-rs`'s own dependency per `Cargo.lock`, but
  should be declared explicitly rather than relied on incidentally).
- `assets/views/home/index.html` — add Login/Register/Dashboard links next
  to the existing `/hello` link, so the new pages are discoverable (mirrors
  goravel's navbar showing Login/Register when logged out).

## Verification

1. `cargo check --all-features` / `cargo check --all-features --tests`
2. `cargo test --all-features --all` — all existing 28 + the new
   `auth_web` tests passing (confirms the JSON API's Bearer-based
   `/api/auth/*` flow is untouched by this addition)
3. `cargo fmt --all -- --check` and the CI clippy command
   (`-D warnings -W clippy::pedantic -W clippy::nursery -W rust-2018-idioms`)
4. `cargo loco routes` — confirm `/login`, `/register`, `/logout`,
   `/dashboard` appear alongside the untouched `/api/auth/*` group
5. Manual smoke test: `cargo loco start`, register a user through
   `/register` in a browser, confirm redirect to `/dashboard` shows the
   user, log out, log back in through `/login`, hit `/dashboard` directly
   while logged out and confirm the redirect to `/login`
