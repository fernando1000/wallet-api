# wallet-api

A wallet and payments REST API in Rust, built step by step in a YouTube series. The series starts with a single Axum route and grows toward a production backend with users, accounts, balances and transfers. Each lesson adds the Rust and backend concepts the API needs at that point.

Every lesson has its own folder with the complete code at the end of that video, so each one builds and runs on its own. Each folder has a README with a demo GIF and a link to the video.

## Lessons

| # | Folder | Video | What it covers |
|---|---|---|---|
| 1 | [`01-axum-rest-api`](01-axum-rest-api) | [REST API in Rust with Axum and Tokio](https://youtu.be/W0rsHjBSe4E) (28 min) | Async `main` with Tokio, Axum `Router`, JSON with Serde, `POST /users` returning `201` / `400` |
| 2 | [`02-crud-shared-state`](02-crud-shared-state) | [Full CRUD with Shared State](https://youtu.be/lFmGxQ6NGCk) (15 min) | In-memory users CRUD, `Arc<RwLock<...>>` with `with_state`, the `State` / `Path` / `Query` / `Json` extractors, status codes, `#[debug_handler]`, 100 parallel requests |
| 3 | [`03-error-handling`](03-error-handling) | [Error Handling Done Right](https://youtu.be/gk7aUfa3HL4) (14 min) | One `ApiError` enum with `thiserror`, `IntoResponse` with the same JSON error body, `?` and `From` (including `From<PoisonError<T>>`), custom extractors with `#[derive(FromRequest)]` for Axum's rejections, `409 Conflict` for duplicate emails, no `unwrap()` in handlers |
| 4 | [`04-validation`](04-validation) | [Validation and Type-Safe DTOs](https://youtu.be/o4VHyqKAC1U) (14 min) | DTO vs domain types, newtypes with private fields (`UserName`, `Email`), `TryFrom` as the only constructor ("parse, don't validate"), `422` listing every invalid field at once, normalized emails so duplicates can't hide, the `validator` crate |
| 5 | [`05-postgres-sqlx`](05-postgres-sqlx) | [PostgreSQL with SQLx](https://youtu.be/XPcp-o8gmqs) (18 min) | Postgres in Docker Compose, versioned migrations with the SQLx CLI and `migrate!()`, a `PgPool` instead of `Arc<RwLock<...>>`, queries checked at compile time with `query_as!` / `query!`, `Option` from `fetch_optional`, newtypes read with `#[sqlx(transparent)]`, `UNIQUE` email with unique violations mapped to `409` |
| 6 | [`06-repository-pattern`](06-repository-pattern) | [The Repository Pattern](https://youtu.be/zbglU7Ppv10) (20 min) | A `UserRepository` trait with its own `RepoError` (converted with `From`), the SQLx code moved into `PgUserRepository`, handlers generic over the trait, why `Arc<dyn Trait>` fails with `async fn` (E0038), generics vs `dyn`, `impl Future + Send` in the trait, an in-memory fake repository and `#[tokio::test]` handler tests without a database |
| 7 | [`07-integration-tests`](07-integration-tests) | [Integration Tests for Axum](https://youtu.be/W_qXh1a5BVY) (17 min) | The crate split into `lib.rs` + `main.rs` so `tests/` can import it, real HTTP requests sent into the `Router` with `tower::ServiceExt::oneshot` (no server, no open port), a throwaway `postgres:18` per test with testcontainers, the container's lifetime and `Drop`, `[dev-dependencies]`, unit and integration tests in one `cargo test` |
| 8 | [`08-config-logging`](08-config-logging) | [Config and Structured Logging](https://youtu.be/M8NT8bw6oCg) (18 min) | A typed `Config` read once from the environment and validated at startup (fail fast), an `AppEnv` enum, config tests with a `HashMap` through a generic closure (`set_var` is `unsafe` in edition 2024), structured logs with `tracing` and `tracing-subscriber` (text in development, JSON in production), a request id in the `x-request-id` header and in every log line with `tower-http`, layer order with `ServiceBuilder` |

Each lesson is also tagged (`lesson-01`, `lesson-02`, ...), so you can check out the repository as it was after that video.

## Running a lesson

Requirements: Rust 1.85 or newer (edition 2024). From lesson 5 on, also Docker with Compose and the SQLx CLI (`cargo install sqlx-cli`). From lesson 7 on, `cargo test` also needs Docker running, because each integration test starts its own Postgres container.

```bash
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/08-config-logging
docker compose up -d
sqlx migrate run
cargo test
cargo run
```

The API listens on `http://localhost:8080`. The README in each folder lists the requests to try.

## Stack

[Axum](https://github.com/tokio-rs/axum) 0.8 · [Tokio](https://tokio.rs/) 1 · [Serde](https://serde.rs/) 1 · [thiserror](https://github.com/dtolnay/thiserror) 2 (from lesson 3) · [validator](https://github.com/Keats/validator) 0.21 (from lesson 4) · [SQLx](https://github.com/launchbadge/sqlx) 0.9 with PostgreSQL 18 (from lesson 5) · [testcontainers](https://github.com/testcontainers/testcontainers-rs-modules-community) 0.15 and [tower](https://github.com/tower-rs/tower) 0.5 in the tests (from lesson 7) · [tracing](https://github.com/tokio-rs/tracing) 0.1 and [tower-http](https://github.com/tower-rs/tower-http) 0.7 (from lesson 8)

---

Made by Fernando Santos · [YouTube](https://youtu.be/M8NT8bw6oCg) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
