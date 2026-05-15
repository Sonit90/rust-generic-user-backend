# price-merger

Rust backend for merging and converting price-list files (`.csv`, `.xls`, `.xlsx`)
with per-column and global transformations.

Cargo workspace layout:

```
crates/
├── api/             Axum HTTP layer
├── core/            Domain types (no HTTP, no DB)
├── db/              SQLx + Postgres
├── auth/            JWT, Argon2, OAuth (Google), RBAC
├── file-processor/  CSV/XLS/XLSX readers, transforms, writers
└── jobs/            Postgres-backed worker pool (SKIP LOCKED)
migrations/   SQL migrations
config/       default.toml + production.toml + test.toml
docker/       Dockerfile + docker-compose.yml
```

## Prerequisites

- Rust 1.95+ (`rust-toolchain.toml` pins it)
- Docker Desktop (for Postgres + RustFS) — must be **running** before any `docker compose` command
  - `sqlx-cli` for migrations and offline prep:
    ```powershell
    cargo install sqlx-cli --no-default-features --features rustls,postgres
    ```
    To launch app in watch mode
    ```powershell
    cargo watch -x "run -p price-merger-api"
  ```

## First-time setup

```powershell
# 1. Copy env template
Copy-Item .env.example .env

# 2. Start Docker Desktop, then bring up the dependencies
cd docker
docker compose up -d postgres rustfs rustfs-init
cd ..

# 3. Run migrations against the running Postgres
$env:DATABASE_URL = "postgres://price_merger:price_merger@localhost:5432/price_merger"
sqlx migrate run

# 4. (Optional) Generate offline SQLx query data so the `api` image
#    can build without a DB connection. Commit the resulting .sqlx/ folder.
cargo sqlx prepare --workspace
```

## Run modes

### A. Local dev — DB & storage in Docker, API native (fastest iteration)

```powershell
cd docker
docker compose up -d postgres rustfs rustfs-init
cd ..

$env:DATABASE_URL = "postgres://price_merger:price_merger@localhost:5432/price_merger"
$env:RUST_LOG     = "info,price_merger=debug,sqlx=warn"
cargo run -p price-merger-api
```

API listens on `http://localhost:8080`. Health check: `GET /health`.

### B. Everything in Docker

Requires `cargo sqlx prepare` to have been run first (so `SQLX_OFFLINE=true`
in the Dockerfile has data to use), and the `.sqlx/` directory committed.

```powershell
cd docker
docker compose up -d --build
```

### C. Stop / reset

```powershell
cd docker
docker compose down               # stop containers, keep volumes
docker compose down -v            # also wipe Postgres + RustFS data
```

## Bootstrapping the first admin

There is no admin user seeded by migrations and no open HTTP endpoint to self-promote. Use the built-in CLI command instead:

```powershell
$env:DATABASE_URL = "postgres://price_merger:price_merger@localhost:5432/price_merger"
cargo run -p price-merger-api -- create-admin --email=you@example.com --password=yourpassword
```

Or with the compiled binary:

```powershell
./price-merger-api create-admin --email=you@example.com --password=yourpassword
```

The command connects directly to the database, hashes the password with Argon2, and inserts the user with the `admin` role. No running server needed. Log in via `POST /api/v1/auth/login` afterward.

## API documentation (Swagger UI)

Swagger UI is served by the running API — no separate generation step needed.

| URL | Description |
|-----|-------------|
| `http://localhost:8080/api/docs` | Interactive Swagger UI |
| `http://localhost:8080/api/docs/openapi.json` | Raw OpenAPI 3.1 JSON |

Start the server first (see "Run modes"), then open the UI in a browser. Use the **Authorize** button to paste a Bearer token obtained from `POST /api/v1/auth/login`.

## API endpoints (v1)

Auth header for all protected routes: `Authorization: Bearer <access_token>`.

### Health

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/health` | — | Liveness check |

### Authentication

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/auth/register` | — | Register with email + password; returns access + refresh tokens |
| POST | `/api/v1/auth/login` | — | Email/password login; returns access + refresh tokens |
| POST | `/api/v1/auth/refresh` | — | Rotate refresh token; returns new access + refresh tokens |
| POST | `/api/v1/auth/logout` | Bearer | Revoke refresh token |
| GET | `/api/v1/auth/verify-email` | — | Verify email address via `?token=` query param |
| POST | `/api/v1/auth/resend-verification` | Bearer | Resend verification email to current user |
| POST | `/api/v1/auth/forgot-password` | — | Send password-reset email (always 200 to prevent enumeration) |
| POST | `/api/v1/auth/reset-password` | — | Reset password with token + new password |
| GET | `/api/v1/auth/oauth/google` | — | Redirect to Google OAuth |
| GET | `/api/v1/auth/oauth/google/callback` | — | Google OAuth callback; exchanges code for tokens |

### Users

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/users/me` | Bearer | Current user profile |
| GET | `/api/v1/users/me/permissions` | Bearer | Current user's effective permissions |

### Files

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/files` | Bearer | Multipart upload (field `file`); validates size, computes SHA-256 |
| GET | `/api/v1/files` | Bearer | List own files (limit 100) |
| GET | `/api/v1/files/{id}` | Bearer | Download file (owner or `files.read_all`) |
| DELETE | `/api/v1/files/{id}` | Bearer | Enqueue async purge job |
| GET | `/api/v1/files/{id}/preview` | Bearer | Preview headers + rows (query: `sheet`, `rows`) |

### Column Mappings

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/mappings` | Bearer | Create column mapping for a file |
| GET | `/api/v1/mappings` | Bearer | List own mappings |
| GET | `/api/v1/mappings/{id}` | Bearer | Get mapping (owner or `mappings.manage_any`) |
| DELETE | `/api/v1/mappings/{id}` | Bearer | Delete mapping (owner or `mappings.manage_any`) |

### Output Formats

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/output-formats` | Bearer | Create reusable output format (columns + expression transforms) |
| GET | `/api/v1/output-formats` | Bearer | List own output formats |
| GET | `/api/v1/output-formats/{id}` | Bearer | Get format (owner or `formats.manage_any`) |
| PATCH | `/api/v1/output-formats/{id}` | Bearer | Update format partially (owner or `formats.manage_any`) |
| DELETE | `/api/v1/output-formats/{id}` | Bearer | Delete format (owner or `formats.manage_any`) |

#### Output format request body fields

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Unique name per user |
| `description` | string? | Optional description |
| `filename` | string? | Base filename (no extension) for the output; UUID used when absent |
| `columns` | `OutputColumn[]` | Output column definitions (unchanged) |
| `output_extension` | `"csv"\|"xls"\|"xlsx"` | Output file format (default `"xlsx"`) |
| `file_filter` | `Expr?` | Per-input filter: row skipped when expression is false/null (evaluated after column mapping, before merge) |
| `global_filter` | `Expr?` | Post-merge filter: row skipped when expression is false/null |
| `transforms` | `ExprTransform[]` | Column transforms applied sequentially after merge: `{field, expr}` |

#### Expression engine (`Expr`)

Expressions are JSON objects with an `"op"` discriminator field:

```json
{"op": "+", "args": [{"op": "var", "field": "price"}, {"op": "num", "value": 10}]}
```

| `op` | Shape | Description |
|------|-------|-------------|
| `num` | `{"op":"num","value":f64}` | Numeric literal |
| `str` | `{"op":"str","value":string}` | String literal |
| `bool` | `{"op":"bool","value":bool}` | Boolean literal |
| `null` | `{"op":"null"}` | Null literal |
| `var` | `{"op":"var","field":string}` | Read field from current row |
| `+` `-` `*` `/` | `{"op":"…","args":[expr,expr]}` | Arithmetic (non-numeric → null; `/0` → null) |
| `>` `>=` `<` `<=` | `{"op":"…","args":[expr,expr]}` | Numeric comparison → bool |
| `==` `!=` | `{"op":"…","args":[expr,expr]}` | Text comparison → bool |
| `and` `or` | `{"op":"…","args":[expr,…]}` | Logical, ≥2 args, short-circuits |
| `!` | `{"op":"!","arg":expr}` | Logical not |
| `if` | `{"op":"if","cond":expr,"then":expr,"else":expr}` | Conditional |
| `in` | `{"op":"in","value":expr,"items":[expr,…]}` | Text membership check |
| `concat` | `{"op":"concat","args":[expr,…]}` | String concatenation |
| `round` | `{"op":"round","value":expr,"decimals":expr}` | Round to N decimal places |

Limits: max depth 20, max 100 nodes per expression. Validated at format-create time.

### Merge Runs

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| POST | `/api/v1/merge/runs` | Bearer | Start merge job (N input files → 1 output) |
| GET | `/api/v1/merge/runs` | Bearer | List own merge runs |
| GET | `/api/v1/merge/runs/{id}` | Bearer | Poll run status (owner only) |
| GET | `/api/v1/merge/runs/{id}/download` | Bearer | Download completed output (status must be `completed`) |

### Admin

All admin endpoints require admin-level permissions noted in the Description column.

| Method | Path | Auth | Description |
|--------|------|------|-------------|
| GET | `/api/v1/admin/users` | Bearer | List all users paginated (`limit`/`offset`); requires `users.read` |
| PATCH | `/api/v1/admin/users/{id}/role` | Bearer | Change user role; requires `users.assign_roles` |
| PATCH | `/api/v1/admin/users/{id}/active` | Bearer | Enable/disable account; requires `users.manage` |
| POST | `/api/v1/admin/users/{id}/permissions` | Bearer | Grant or revoke a permission; requires `users.assign_roles` |

## Configuration

Values are loaded in this order (later overrides earlier):

1. `config/default.toml`
2. `config/<APP_ENV>.toml` (e.g. `production.toml`)
3. Environment variables of the form `APP__SECTION__KEY` (double underscores delimit nesting)
4. `DATABASE_URL` (overrides `db.url`)

See `.env.example` for the full list.

## Running tests

Unit tests in the `api` crate (e.g. for `POST /api/v1/auth/register`) use a
mocked `RegisterPort` and don't need a database — they run purely in-process:

```powershell
# Just the auth tests
cargo test -p price-merger-api routes::auth

# All api crate tests
cargo test -p price-merger-api

# Whole workspace
cargo test --workspace
```

`SQLX_OFFLINE=true` is honored automatically via the committed `.sqlx/`
cache, so `cargo test` works without a running Postgres for any crate that
only uses mocks. Crates that touch the real DB will still need
`DATABASE_URL` and `sqlx migrate run` — see "First-time setup" above.

## Common tasks

```powershell
# Workspace build / test
cargo build --workspace
cargo test  --workspace

# Lint
cargo clippy --workspace --all-targets

# Add a new migration
sqlx migrate add my_migration_name

# Rebuild offline SQLx cache after changing queries
cargo sqlx prepare --workspace

# Tail API logs in container mode
cd docker; docker compose logs -f api
```

## Troubleshooting

**`error during connect ... dockerDesktopLinuxEngine`** — Docker Desktop isn't
running. Start it from the Start menu and wait for "Engine running".

**`error: relation "..." does not exist`** when running tests — migrations
haven't been applied to the test DB. Run `sqlx migrate run` against the
`price_merger_test` database.

**`SQLX_OFFLINE=true` but no cached data** during Docker build — run
`cargo sqlx prepare --workspace` while a dev Postgres is up, and commit
the generated `.sqlx/` directory.

**`error: error returned from database: relation "..." does not exist`**
when running `cargo build` or `cargo sqlx prepare` — the `sqlx::query!`
macros introspect the live DB at compile time, so migrations must be applied
first. Run:

```powershell
$env:DATABASE_URL = "postgres://price_merger:price_merger@localhost:5432/price_merger"
sqlx migrate run --source migrations
cargo sqlx prepare --workspace
```

**`password authentication failed for user "price_merger"`** — usually means
either `DATABASE_URL` isn't set in the current shell, or another Postgres
(native Windows install or stale container) is squatting on port 5432.
Check with `netstat -ano | findstr ":5432"` — if you see two listeners,
stop the native one (`Get-Service *postgres* | Stop-Service`) or remap the
Docker port to `5433:5432` in `docker/docker-compose.yml`.

**Two Postgres instances on 5432** — Windows often has a native Postgres
running as a service. Either stop it, or change the Docker host port to
`5433` and update `DATABASE_URL` accordingly.

**RustFS bucket missing** — the `rustfs-init` one-shot service creates it on
first boot. If you wiped volumes, just run
`docker compose up -d rustfs-init` again.
