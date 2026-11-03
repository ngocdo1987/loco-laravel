# Loop State — goravel → loco migration

Queue for porting the blog module from `goravel_base` (Go/Goravel) into this
repo (Rust/Loco). Module structure and auth are already done — see "Done".

- Pattern: pr-babysitter (L2, manual trigger) · Cycle: `LOOP.md`
- Source repo (READ-ONLY reference): `/home/ubuntu/projects/GOLANG/goravel_base`
- Rule: one slice = one worktree = one PR = one independent verifier pass.
  Pick slices in order. Split a slice before coding if it can't fit one PR.

## High Priority

Work top-down — S1 first. One slice = one worktree = one PR = one independent verifier pass. Split a slice before coding if it can't fit one PR.

- [ ] S4. Public read API — GET `/api/posts`, `/api/posts/{slug}`,
      `/api/posts/{slug}/comments`, `/api/categories`, `/api/tags`.
      Source: `app/modules/blog/routes.go` (public block)
- [ ] S5. Author write API (JWT) — POST `/api/posts` + `/api/me/posts` CRUD.
      Source: routes.go protected block; ownership + slug-uniqueness rules in
      `app/modules/blog/services/blog_service.go`
- [ ] S6. Taxonomy write API — categories/tags CRUD (6 endpoints)
- [ ] S7. Comments API — submit (pending), delete-own, moderation queue,
      moderate, delete-moderated (5 endpoints)
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
- Post HTML sanitization (bluemonday in goravel) — choose the Rust equivalent
  during S4/S5; never expose unsanitized HTML from new endpoints
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

## Recent Noise

Examined and ignored this cycle.

- `GET /api/users` (goravel) is tracked as S12, not noise
- Goravel facades/cache/queue/gRPC surface — out of scope for this migration

---
Run log: `loop-run-log.md` (one JSON line per slice). Cadence and gates: `LOOP.md`.
