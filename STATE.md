# Loop State — goravel → loco migration

Queue for porting the blog module from `goravel_base` (Go/Goravel) into this
repo (Rust/Loco). Module structure and auth are already done — see "Done".

- Pattern: pr-babysitter (L2, manual trigger) · Cycle: `LOOP.md`
- Source repo (READ-ONLY reference): `/home/ubuntu/projects/GOLANG/goravel_base`
- Rule: one slice = one worktree = one PR = one independent verifier pass.
  Pick slices in order. Split a slice before coding if it can't fit one PR.

## High Priority

Work top-down — S1 first. One slice = one worktree = one PR = one independent verifier pass. Split a slice before coding if it can't fit one PR.

- [ ] S8. Uploads API — `/api/uploads/images`, `/api/uploads/post-covers`,
      `/api/me/posts/{id}/cover`. Decision: use loco `ctx.storage` (not the
      goravel storage facade). Source: blog_service.go upload paths
- [ ] S9. Web public pages — `/`, `/posts/{slug}`, `/categories/{slug}`,
      `/tags/{slug}` as Tera views under `assets/views/blog/`.
      Source: `resources/views/blog/` (public templates)
- [ ] S10. Web dashboard — posts CRUD + taxonomy + moderation forms
      (~15 endpoints). Split into S10a posts / S10b taxonomy+moderation if the
      diff would exceed gate.yaml `maxFiles`.
- [ ] S11. Port `tests/blog_test.go` → `tests/requests/blog_*.rs` — parity
      contract for S4–S7 (published-only reads, author-only drafts, pending
      comments, moderation rules, sanitization)
- [ ] S12. DECISION — auth parity: goravel has `GET /api/users`; loco auth is
      a deliberate superset otherwise. Record decision here; PR only if "add".

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

## Recent Noise

Examined and ignored this cycle.

- `GET /api/users` (goravel) is tracked as S12, not noise
- Goravel facades/cache/queue/gRPC surface — out of scope for this migration

---
Run log: `loop-run-log.md` (one JSON line per slice). Cadence and gates: `LOOP.md`.
