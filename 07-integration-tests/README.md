# Lesson 7: Integration Tests for a Rust Axum API

Lesson 6 ended with three handler tests that run in milliseconds against an in-memory fake. They never see the routes, the extractors, the JSON of the error responses or the real SQL. This lesson tests the API from the outside. Integration tests in the `tests/` folder send real HTTP requests straight into the `Router` with `tower::ServiceExt::oneshot`, with no server and no open port. They run against a throwaway PostgreSQL 18 that [testcontainers](https://github.com/testcontainers/testcontainers-rs-modules-community) starts in Docker for each test, with the migrations run at the start. To let the tests import the code, the crate becomes a library (`src/lib.rs`) plus a small binary (`src/main.rs`). Built with Rust (edition 2024), [Axum](https://github.com/tokio-rs/axum) 0.8.9, [Tokio](https://tokio.rs/) 1.54.1, [SQLx](https://github.com/launchbadge/sqlx) 0.9.1, PostgreSQL 18, [dotenvy](https://github.com/allan2/dotenvy) 0.15.7, [Serde](https://serde.rs/) 1.0.229, [thiserror](https://github.com/dtolnay/thiserror) 2.0.21 and [validator](https://github.com/Keats/validator) 0.21.0. The tests also use [tower](https://github.com/tower-rs/tower) 0.5.3, testcontainers-modules 0.15.0 and [serde_json](https://github.com/serde-rs/json) 1.0.154.

![Demo: cargo test runs the 3 unit tests from lib.rs in 0.00s, then the 4 integration tests from tests/api.rs (create_and_get_user, duplicate_email_returns_409, missing_user_returns_404, invalid_email_returns_422) against a real Postgres container, all passing in 6.70s](docs/demo.gif)

**Full video tutorial on YouTube (17 min):** https://youtu.be/W_qXh1a5BVY

---

## What it does

The API is the same as in lesson 6: the same five routes, the same JSON and the same status codes. What's new is the test suite. One `cargo test` now runs both kinds of tests:

| Kind | Where | What it calls | Database | Time |
|---|---|---|---|---|
| Unit tests | `#[cfg(test)] mod tests` in [`src/lib.rs`](src/lib.rs) | the handler functions directly | in-memory fake | milliseconds |
| Integration tests | [`tests/api.rs`](tests/api.rs) | the whole `Router`, through HTTP requests | real PostgreSQL 18 in Docker, one per test | seconds |

## How it works: the whole path, without a server

### A library and a binary

Every file in `tests/` is compiled as its own crate. Like any outside user, it can only see the public API of a library, and a binary crate can't be imported at all. So the crate is split in two:

- [`src/lib.rs`](src/lib.rs) has the types, the handlers and `pub fn router<R: UserRepository>(repo: R) -> Router`. `pub mod repository` exposes `PgUserRepository`. `pub` is the line between what the tests can see and what stays inside.
- [`src/main.rs`](src/main.rs) only has `main()`. It uses its own library by the package name, `wallet_api`:

```rust
use wallet_api::{repository::PgUserRepository, router};
```

### One Postgres per test

The helper starts a `postgres:18` container (the same version as `docker-compose.yml`), connects to the random host port Docker maps, runs the migrations and builds the router with the real repository ([`tests/api.rs`](tests/api.rs)):

```rust
async fn spawn_app() -> (Router, ContainerAsync<Postgres>) {
    let container = Postgres::default().with_tag("18").start().await.unwrap();
    let port = container.get_host_port_ipv4(5432).await.unwrap();
    let url = format!("postgres://postgres:postgres@127.0.0.1:{port}/postgres");
    let pool = PgPoolOptions::new()
        .acquire_timeout(Duration::from_secs(3))
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!().run(&pool).await.unwrap();
    (router(PgUserRepository::new(pool)), container)
}
```

The container lives only as long as the `ContainerAsync` value lives (RAII). When the value is dropped, the container is removed. That is why the helper returns it, and each test keeps it in `_container` until the end. A plain `_` would drop it right away. Each test has its own empty database, so the tests never share data and can run in parallel.

### Requests straight into the router

A `Router` is a `tower::Service`. `oneshot` sends it one request and returns the response, with no TCP and no open port:

```rust
let (app, _container) = spawn_app().await;
let response = app.oneshot(get("/users/99")).await.unwrap();
assert_eq!(response.status(), StatusCode::NOT_FOUND);
```

`oneshot` takes the router by value, so a test that sends two requests calls `app.clone().oneshot(...)` for the first one. Cloning a `Router` is cheap. Small helpers build the requests and read the body: `get`, `post_json` (with the `content-type: application/json` header) and `read_json`, which collects the body with `axum::body::to_bytes` into a `serde_json::Value`.

## The errors from the video

1. **``error[E0432]: unresolved import `wallet_api` ``.** `tests/api.rs` tried to import a crate that was still binary only. Fixed by moving the code into `src/lib.rs` and leaving `main()` in `src/main.rs`.
2. **`left: 500`, `right: 404`.** The first version of `spawn_app()` returned only the `Router`. The container was dropped when the helper returned, so the query in `GET /users/99` failed and the API answered `500 internal_error`. Fixed by returning the container too and keeping it in `_container`. The short `acquire_timeout` (3 s) makes this fail fast instead of waiting for SQLx's default 30 s.
3. **`left: 415`, `right: 201`.** `curl -H 'content-type: application/json'` had been setting the header in earlier lessons, but `Request::builder()` doesn't. Axum's `Json` extractor rejected the body with `415 Unsupported Media Type`. Fixed by adding `.header(CONTENT_TYPE, "application/json")` in `post_json`.

## Tests

Integration tests ([`tests/api.rs`](tests/api.rs)), each one against its own container:

| Test | What it proves |
|---|---|
| `missing_user_returns_404` | `GET /users/99` on an empty database returns `404` with `"code": "user_not_found"` in the JSON body |
| `create_and_get_user` | `POST /users` returns `201` with id `1`, and `GET /users/1` returns `200` with the same name |
| `duplicate_email_returns_409` | a second `POST` with `ALICE@EXAMPLE.COM` hits the real `UNIQUE` constraint in Postgres and returns `409` with `"code": "email_taken"` |
| `invalid_email_returns_422` | `POST` with `not-an-email` returns `422` with `"code": "validation_failed"` and the `email` field listed |

The 3 unit tests from lesson 6 (`create_returns_201_with_id`, `duplicate_email_is_taken`, `missing_user_is_not_found`) are unchanged and now live in `src/lib.rs`.

## Project structure

```
07-integration-tests/
├── .env                  # DATABASE_URL for the local Postgres (development credentials only)
├── docker-compose.yml    # PostgreSQL 18 with a named volume
├── Cargo.toml            # dependencies + [dev-dependencies]: serde_json, testcontainers-modules (postgres), tower (util)
├── Cargo.lock
├── docs/demo.gif
├── migrations/
│   └── 20261120171858_create_users.sql   # users table, UNIQUE email
├── src/
│   ├── domain.rs         # UserName and Email newtypes
│   ├── repository.rs     # UserRepository trait, RepoError, PgUserRepository, InMemoryUserRepository (tests only)
│   ├── lib.rs            # ApiError, pub router and handlers, unit tests
│   └── main.rs           # main(): pool, migrations, server on port 8080
└── tests/
    └── api.rs            # integration tests: spawn_app(), request helpers, 4 tests
```

## Running it

Requirements: Rust 1.85 or newer (edition 2024), Docker with Compose and the SQLx CLI 0.9.

```bash
cargo install sqlx-cli
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/07-integration-tests
docker compose up -d
sqlx migrate run
cargo test
```

Postgres from `docker-compose.yml` must be running and the `users` table must exist before `cargo build` or `cargo test`, because the SQLx query macros check the SQL against the database while compiling. The integration tests don't use that database: each one starts its own `postgres:18` container, so Docker must be running. The first run pulls the image and takes longer.

To run only one kind of test:

```bash
cargo test --lib          # unit tests, no Docker needed at run time
cargo test --test api     # integration tests
```

The server still runs as before, on `http://localhost:8080`:

```bash
cargo run
```

The `.env` file holds the local development connection string (`postgres://wallet:wallet@localhost:5432/wallet`), matching `docker-compose.yml`. Don't reuse these credentials outside your machine.

To stop Postgres: `docker compose down` (add `-v` to also delete the data).

## Important note

This project is for learning. So far it only has users: no authentication, accounts or balances yet.

---

Made by Fernando Santos · [YouTube tutorial](https://youtu.be/W_qXh1a5BVY) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
