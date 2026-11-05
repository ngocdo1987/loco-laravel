# Loop State — goravel → loco migration

Queue for porting the blog module from `goravel_base` (Go/Goravel) into this
repo (Rust/Loco). Module structure and auth are already done — see "Done".

- Pattern: pr-babysitter (L2, manual trigger) · Cycle: `LOOP.md`
- Source repo (READ-ONLY reference): `/home/ubuntu/projects/GOLANG/goravel_base`
- Rule: one slice = one worktree = one PR = one independent verifier pass.
  Pick slices in order. Split a slice before coding if it can't fit one PR.

## High Priority

**Queue empty — S1–S12 all merged. Migration complete.** Next step is a
human decision: switch this repo to daily-triage (L1) upkeep per LOOP.md
("After the migration") — keep STATE.md, loop-run-log.md, loop-budget.md.
The pre-existing clippy `--all-targets` debt (Watch List) is the obvious
candidate for the first L1 item.

## Watch List

Monitor only — do not act.

- Pre-existing clippy debt (found by S2 verifier): `cargo clippy
  --all-features --all-targets` fails with 3 `future_not_send` pedantic
  errors in `tests/requests/prepare_data.rs`. The LOOP.md gate (no
  `--all-targets`) passes; clean up in a later slice before it bites

- Goravel's Module interface carries `Dependencies()`/enable-disable +
  `Seeders()`; `docs/modules.md` deliberately deferred them — revisit when
  blog lands (blog genuinely depends on auth)
- DB dialect: goravel=MySQL/uint IDs, loco=Postgres/i64 IDs; `carbon.DateTime`
  → `timestamp with time zone`; map types explicitly in S1
- Goravel server-rendered forms use its own view helpers; port to Tera+i18n
  conventions (`assets/views/`, `assets/i18n/`) in S9/S10

## Done

- [x] Module structure + registry + module convention docs (commit 2c3a737)
- [x] Auth module complete: password/magic-link/forgot/reset + web UI +
      mailer + task + tests (commits 791596f, 76e9eeb) — superset of goravel auth
- [x] S1. Blog schema migration — posts, categories, tags, post_tags (m2m
      composite PK), comments; entities regenerated; FK parity verified on
      sqlite scratch (RESTRICT category/tag, CASCADE rest); verifier APPROVE
      (commit f1a2b0c). Notes for S2: post_tags has no single-column PK;
      loco 1.1 `db entities` now also writes `src/models/*.rs` wrappers (users
      wrapper is a re-export shim — auth owns its ActiveModelBehavior)
- [x] S2. Blog models — Category/Tag finders, Post draft|published +
      relations loading (PostWithRelations), Comment pending|approved|
      rejected, response DTOs with goravel JSON parity; 3 model tests;
      verifier APPROVE, loop-gate ALLOWED (commit d484e35)
- [x] S3. Blog fixtures/seeders — 5 categories, 6 tags, 3 posts, 8 post-tag
      links, 4 comments and 3 demo users matching Goravel; transactional,
      repeatable blog seeding through the Loco seed hook. Independent verifier
      APPROVE; 41 MySQL tests and 5 SQLite fixture tests passed; migration,
      unchanged routes, fmt, clippy and loop-gate passed (commit 3b047ea).
      Merged directly into master at the user's explicit request.

- [x] S4. Public read API — five endpoints, filtering/pagination, published-only
      posts and approved-only comments; response HTML sanitized with ammonia
      4.2.4. Independent verifier APPROVE; all 45 MySQL tests, fmt, clippy,
      clean MySQL migration and routes passed. Cargo dependency gate exception
      explicitly approved by the user. Commit 57d309e; merged directly into
      master at the user's explicit request.

- [x] S5. Author write API (JWT) — create + own-posts CRUD, ownership,
      slug uniqueness, transactional tags and HTML sanitization before saving.
      MySQL-only LONGTEXT migration supports 100,000 characters and guards
      lossy rollback. Independent verifier APPROVE; all 52 MySQL tests, fmt,
      clippy, clean MySQL migration and routes passed. Migration exception and
      maxFiles increase to 20 explicitly approved; 9 implementation files.
      Commit a1bc4a4; merged directly into master at the user's explicit request.

- [x] S6. Taxonomy write API — six JWT create/update/delete endpoints for
      categories/tags, name validation and slug conflicts; atomic RESTRICT
      protection for taxonomy assigned to published or draft posts. Shared
      slug normalization matches Go Unicode behavior. Independent verifier
      APPROVE; all 58 MySQL tests, fmt, clippy, clean MySQL migration, routes
      and loop-gate passed; 5 implementation files, no denylist exceptions.
      Commit ae1350c; merged directly into master at the user's explicit request.

- [x] S7. Comments API — five JWT endpoints: submit (pending start,
      published-post-only, trimmed ≤2000-char content), delete-own, owner
      moderation queue (status filter + pagination), moderate
      (approved/rejected), delete-moderated; goravel parity for status codes,
      404 ownership and validation error shapes. Independent verifier APPROVE;
      all 62 MySQL tests, fmt, clippy, clean MySQL migration, routes (+5, none
      lost) and loop-gate passed; 5 files, no denylist exceptions (MySQL
      config kept uncommitted in worktrees, as in S4–S6). Commit f4060a9;
      merged directly into master at the user's explicit request.

- [x] S8. Uploads API — three JWT endpoints: `/api/uploads/images` and
      `/api/uploads/post-covers` (field `image`) plus
      `/api/me/posts/{id}/cover` (field `cover_image`, persists
      cover_image_url on the owned post). Loco storage wired in code via
      `after_context` (local driver rooted at `storage/app/public`) since
      `config/*.yaml` is denylisted; `DefaultBodyLimit` initializer (8 MB)
      lifts axum's 2 MB multipart cap; content-sniffed jpeg/png/gif/webp
      before the 5 MB check; url = `{proto}://{host}/storage/{path}`;
      uuid filenames; `storage/` gitignored. Independent verifier APPROVE;
      all 65 MySQL tests, fmt, clippy, clean MySQL migration, routes (+3,
      none lost) and loop-gate passed; 8 files, no denylist exceptions.
      Serving `/storage/*` statics deferred to S9/S10 (goravel wires it as a
      web-level static route, not module API). Commit 198a21b; merged
      directly into master at the user's explicit request.

- [x] S9. Web public pages — four Tera views under `assets/views/blog/`
      (home/feed/post) served by `modules/blog/web.rs`: `/` blog home
      (9-per-page view limit, search, pagination, sidebar),
      `/categories/{slug}` + `/tags/{slug}` feeds (200 empty state for
      unknown slugs), `/posts/{slug}` with ammonia-sanitized content,
      approved-only comments and a cookie-auth (`auth_token` JWT) comment
      form; `controllers.rs` feed/approved-comments logic extracted into
      shared helpers (API behavior unchanged); starter `/` route replaced by
      blog home (`/hello` kept, dead index.html removed). Independent
      verifier APPROVE; all 68 MySQL tests, fmt, clippy, clean MySQL
      migration, routes (+3 web, none lost) and loop-gate passed; 10 files,
      no denylist exceptions. Deferred to S10: web comment submit
      (`POST /posts/{slug}/comments` renders but 404s) and goravel's
      per-handler "Invalid category/tag query." messages (cosmetic;
      verifier note — status/page shape already match). Commit e1cb383;
      merged directly into master at the user's explicit request.

- [x] S10. Web dashboard — 17 `/dashboard/**` routes in
      `modules/blog/dashboard.rs`: posts CRUD (multipart form + optional
      cover upload through the S8 storage path), taxonomy
      create/rename/delete, moderation queue + approve/reject/delete; plus
      the S9-deferred `POST /posts/{slug}/comments` (pending queue, fixed
      goravel validation/error flash texts), the S8-deferred
      `GET /storage/{*path}` static serving (traversal-guarded), and the S9
      cosmetic fix (per-handler "Invalid feed/category/tag query."
      messages). Flash rides two cookies (`blog_flash`/`blog_error`, set on
      redirect, expired on next render) — loco has no session; forms carry
      no CSRF token (frozen auth module owns the session surface). Multipart
      post form is folded into the API's `PostInput::parse` so validation
      rules have exactly one implementation; dashboard writes reuse the
      author API's transactional writers, taxonomy `save`/`remove`, and the
      comment query helpers extracted into `controllers.rs` (API behavior
      byte-identical). Deliberate deviation: `GET /dashboard` itself stays
      the frozen auth module's post-login landing page (it already owned the
      route; dashboard lives under `/dashboard/posts` etc.). One field error
      per form re-render (single-error parser) vs goravel's per-field map.
      Independent verifier APPROVE; all 77 MySQL tests, fmt, clippy, clean
      MySQL migration, routes (+19, none lost) and loop-gate passed; 14
      files, no denylist exceptions. Commit e931488; merged directly into
      master at the user's explicit request.

- [x] S11. Parity contract — `tests/blog_test.go` ported to
      `tests/requests/blog_parity.rs`: the full goravel flow (401 anonymous,
      image + cover uploads, multipart create with inline cover, sanitized
      create, draft 404, publish, filtered feed, pending comment hidden
      until approved, taxonomy delete 409) plus a cover-only multipart
      PATCH test, against the parity suite's own users (created in DB with
      the same hash as Goravel's UserSeeder; loco `/api/auth/login` stands
      in for goravel `/api/login` — frozen auth module shape). Porting it
      exposed a real gap: goravel's write endpoints bind `PostWriteRequest`
      from `form` + `json` tags alike, so `POST /api/posts` and
      `PATCH /api/me/posts/{id}` also accept multipart bodies with an
      optional `cover_image` file; `author.rs` now dispatches on
      content-type (JSON path unchanged — same `PostInput::parse`),
      collecting the form through S10's shared `dashboard::post_form` and
      uploading through `uploads::store_image`; a cover file marks
      `cover_image_url` present before parsing so cover-only patches pass
      the at-least-one-field gate (goravel's `File("cover_image")` check).
      Independent verifier APPROVE (assertion-by-assertion comparison with
      the Go suite; PNG fixture byte-identical); all 79 MySQL tests, fmt,
      clippy, clean MySQL migration, routes (identical — no new routes) and
      loop-gate passed; 5 files, no denylist exceptions. Commit 8fff050;
      merged directly into master at the user's explicit request.

- [x] S12. DECISION — auth parity, resolved WITHOUT a PR (per this slice's
      own rule "PR only if 'add'"). Evidence: goravel's `GET /api/users`
      (`AuthController.Index`) is a scaffold stub returning a static
      `{"Hello": "Goravel"}` JSON — no user data, no auth middleware, used
      by nothing in the goravel codebase. Loco's frozen auth module remains
      the deliberate superset (register/verify/login/forgot/reset/current/
      magic-link×2/resend + web pages + JWT cookie); the only surface
      difference is that stub. Decision (human, via decision handshake):
      **do not add** — porting a hello-world placeholder has no value and
      the natural home would be the denylisted `src/modules/auth/**`.
      Decision-only slice: no worktree, no code changes, no verifier run.



## Recent Noise

Examined and ignored this cycle.

- `GET /api/users` (goravel) is tracked as S12, not noise
- Goravel facades/cache/queue/gRPC surface — out of scope for this migration

---
Run log: `loop-run-log.md` (one JSON line per slice). Cadence and gates: `LOOP.md`.
