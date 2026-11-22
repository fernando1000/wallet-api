# Lesson 6: The Repository Pattern in a Rust Axum API

Lesson 5 ended with five handlers that each ran SQL directly with `query_as!` / `query!`. They worked, but a test for `get_user` needed a running Postgres. This lesson puts the data layer behind a trait. A `UserRepository` trait is the contract for storage, with its own `RepoError` that converts into the HTTP error through `From`. The SQLx code from lesson 5 moves into a `PgUserRepository`, and the handlers become generic over the trait, so they no longer know anything about SQL. An in-memory fake repository then lets `#[tokio::test]` call the handlers directly, with no database, in milliseconds. For the client nothing changes: same routes, same JSON, same status codes, still on PostgreSQL. Built with Rust (edition 2024), [Axum](https://github.com/tokio-rs/axum) 0.8.9, [Tokio](https://tokio.rs/) 1.54.1, [SQLx](https://github.com/launchbadge/sqlx) 0.9.1, PostgreSQL 18, [dotenvy](https://github.com/allan2/dotenvy) 0.15.7, [Serde](https://serde.rs/) 1.0.229, [thiserror](https://github.com/dtolnay/thiserror) 2.0.21 and [validator](https://github.com/Keats/validator) 0.21.0.

![Demo: cargo test after a failing run now shows 3 passed without a database, then cargo run starts the server on Postgres, curl creates Bob (201), lists Alice and Bob, and a second POST with BOB@EXAMPLE.COM gets a 409 email_taken through the repository](docs/demo.gif)

**Full video tutorial on YouTube (20 min):** https://youtu.be/zbglU7Ppv10

---

## What it does

Same five routes and the same JSON as lesson 5. Each handler now calls one repository method:

| Method | Route | Repository call | Success |
|---|---|---|---|
| `POST` | `/users` | `repo.create(new_user)` | `201 Created` + the new user |
| `GET` | `/users` | `repo.list(params.name)` (optional `?name=` filter) | `200 OK` + list |
| `GET` | `/users/{id}` | `repo.find(id)` | `200 OK` + user |
| `PUT` | `/users/{id}` | `repo.update(id, new_user)` | `200 OK` + updated user |
| `DELETE` | `/users/{id}` | `repo.delete(id)` | `204 No Content` |

## How it works: a trait between the handlers and the database

The trait is the contract every storage must follow ([`src/repository.rs`](src/repository.rs)):

```rust
pub trait UserRepository: Clone + Send + Sync + 'static {
    fn create(&self, new_user: NewUser) -> impl Future<Output = Result<User, RepoError>> + Send;
    fn list(
        &self,
        name: Option<String>,
    ) -> impl Future<Output = Result<Vec<User>, RepoError>> + Send;
    fn find(&self, id: i64) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn update(
        &self,
        id: i64,
        new_user: NewUser,
    ) -> impl Future<Output = Result<Option<User>, RepoError>> + Send;
    fn delete(&self, id: i64) -> impl Future<Output = Result<bool, RepoError>> + Send;
}
```

- The supertraits `Clone + Send + Sync + 'static` are what Axum asks of its state: it clones the repository for each request and shares it between Tokio's threads.
- Every method takes `&self`. The repository is shared, so it never needs to be mutable.
- `PgUserRepository` holds the `PgPool` and has the same SQL as lesson 5. Its methods are plain `async fn`.

The handlers are generic over `R: UserRepository`, and the router is built by a generic function:

```rust
fn router<R: UserRepository>(repo: R) -> Router {
    Router::new()
        .route("/users", post(create_user::<R>).get(list_users::<R>))
        .route(
            "/users/{id}",
            get(get_user::<R>)
                .put(update_user::<R>)
                .delete(delete_user::<R>),
        )
        .with_state(repo)
}

async fn get_user<R: UserRepository>(
    State(repo): State<R>,
    AppPath(id): AppPath<i64>,
) -> Result<Json<User>, ApiError> {
    let user = repo.find(id).await?;
    user.map(Json).ok_or(ApiError::UserNotFound(id))
}
```

Axum can't infer `R` from the state, so each handler is named with its type using the turbofish (`create_user::<R>`). `main` picks the real repository once, at startup: `router(PgUserRepository::new(pool))`.

### Generics or `dyn`?

The first try in the video was a trait object, `type AppState = Arc<dyn UserRepository>`. It doesn't compile while the trait has `async fn`: every implementation returns a different future type, even of a different size, and a vtable can't describe that. There are two ways out:

- **`dyn Trait` (dynamic dispatch):** box each future on the heap so they all have one type. That is what the [`async-trait`](https://github.com/dtolnay/async-trait) crate does for you.
- **Generics (static dispatch):** the compiler knows the real type and builds one copy of each handler per repository type, with no cost at run time.

The repository is chosen once, at startup, so this lesson uses generics. Clone is one more thing a trait object couldn't have.

### `async fn` in a trait and `Send`

With generic handlers, a second error came up. Axum only accepts a handler whose future is `Send`, because Tokio moves futures between threads. An `async fn` in a trait makes no promise about `Send`, so for a generic `R` the compiler can't assume it. The fix is in the trait: each method returns `impl Future<Output = ...> + Send`. That is the same signature an `async fn` has, with one promise added. The implementations keep writing `async fn`, and the compiler rejects any implementation whose future isn't `Send`.

## Error handling

The data layer has its own error and knows nothing about HTTP:

```rust
#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("email {0} is already taken")]
    EmailTaken(String),
    #[error("database error")]
    Database(#[from] sqlx::Error),
}
```

`ApiError` converts it with `From`, so `?` in a handler turns a `RepoError` into the right status:

```rust
impl From<RepoError> for ApiError {
    fn from(err: RepoError) -> Self {
        match err {
            RepoError::EmailTaken(email) => ApiError::EmailTaken(email),
            RepoError::Database(err) => ApiError::Database(err),
        }
    }
}
```

The status codes are the same as in lesson 5: `422 validation_failed`, `409 email_taken`, `404 user_not_found`, `400 invalid_path`, `400` / `415` / `422 invalid_json` and `500 internal_error`.

## The errors from the video

1. **`error[E0038]: the trait UserRepository is not dyn compatible ... because method create is async`.** `Arc<dyn UserRepository>` with `async fn` in the trait. Fixed by switching to generics.
2. **``error[E0277]: the trait bound `fn(State<R>, _) -> _ {create_user::<R>}: Handler<_, _>` is not satisfied``.** The generic handlers' futures weren't `Send`. Fixed with `impl Future<Output = ...> + Send` in the trait. (Axum's `#[debug_handler]` from lesson 2 was removed here: it doesn't support generic functions.)
3. **`test result: FAILED. 2 passed; 1 failed`.** The fake repository had no unique email rule, so `duplicate_email_is_taken` failed. A fake that is easier to please than the real database makes tests pass that should fail. Fixed by giving the fake the same rule (below).

## Tests

`InMemoryUserRepository` keeps the users in an `Arc<Mutex<Vec<User>>>` and only exists in tests (`#[cfg(test)]`). It keeps the same promise as the `UNIQUE` constraint in Postgres, checking the email while it holds the lock:

```rust
let mut users = self.users.lock().unwrap();
if users.iter().any(|user| user.email == new_user.email) {
    return Err(RepoError::EmailTaken(new_user.email.as_str().to_string()));
}
```

The tests call the handlers like any function, building the extractors themselves. No server, no HTTP, no database:

```rust
#[tokio::test]
async fn missing_user_is_not_found() {
    let repo = InMemoryUserRepository::default();
    let result = get_user(State(repo), AppPath(99)).await;
    assert!(matches!(result, Err(ApiError::UserNotFound(99))));
}
```

| Test | What it proves |
|---|---|
| `create_returns_201_with_id` | `create_user` returns `201 Created` and the first user gets id `1` |
| `duplicate_email_is_taken` | creating Alice twice (the second time as `ALICE@EXAMPLE.COM`) returns `ApiError::EmailTaken`, the `409` |
| `missing_user_is_not_found` | `get_user` for id `99` in an empty repository returns `ApiError::UserNotFound(99)`, the `404` |

Each test builds its own empty repository, so tests never share data and can run in parallel. The build still needs Postgres, because the SQLx macros check the queries while compiling, but the tests themselves never use it.

## Project structure

```
06-repository-pattern/
├── .env                  # DATABASE_URL for the local Postgres (development credentials only)
├── docker-compose.yml    # PostgreSQL 18 with a named volume
├── Cargo.toml            # axum, dotenvy, serde, sqlx (postgres, macros, migrate), thiserror, tokio, validator
├── Cargo.lock
├── docs/demo.gif
├── migrations/
│   └── 20261120171858_create_users.sql   # users table, UNIQUE email
└── src/
    ├── domain.rs         # UserName and Email newtypes (unchanged from lesson 5)
    ├── repository.rs     # UserRepository trait, RepoError, PgUserRepository, InMemoryUserRepository (tests only)
    └── main.rs           # ApiError (+ From<RepoError>), generic router and handlers, handler tests
```

## Running it

Requirements: Rust 1.85 or newer (edition 2024), Docker with Compose, the SQLx CLI 0.9 and `curl` to test.

```bash
cargo install sqlx-cli
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/06-repository-pattern
docker compose up -d
sqlx migrate run
cargo test
cargo run
```

Postgres must be running and the `users` table must exist before `cargo build`, `cargo test` or `cargo run`, because the query macros check the SQL against the database while compiling.

In a second terminal:

```bash
curl -i -X POST localhost:8080/users -H 'content-type: application/json' -d '{"name":"Bob","email":"bob@example.com"}'   # 201
curl -i localhost:8080/users                                                                                   # 200, list
curl -i -X POST localhost:8080/users -H 'content-type: application/json' -d '{"name":"Bob","email":"BOB@EXAMPLE.COM"}'   # 409 email_taken
```

The API listens on `http://localhost:8080`. The `.env` file holds the local development connection string (`postgres://wallet:wallet@localhost:5432/wallet`), matching `docker-compose.yml`. Don't reuse these credentials outside your machine.

To stop Postgres: `docker compose down` (add `-v` to also delete the data).

## Important note

This project is for learning. So far it only has users: no authentication, accounts or balances yet.

---

Made by Fernando Santos · [YouTube tutorial](https://youtu.be/zbglU7Ppv10) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
