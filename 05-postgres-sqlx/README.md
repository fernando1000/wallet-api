# Lesson 5: PostgreSQL with SQLx in a Rust Axum API

Lesson 4 ended with an API whose users still lived in a `HashMap` in memory: create Alice, restart the server, and `GET /users` returns `[]`. This lesson moves them into PostgreSQL. Postgres runs in Docker Compose, the `users` table comes from a versioned SQLx CLI migration, the `Arc<RwLock<...>>` state is replaced by a `PgPool`, and the five handlers are rewritten with queries that are checked against the real database at compile time (`query_as!`, `query!`). For the client nothing changes: same routes, same JSON, same status codes. But the data now survives a restart. Built with Rust (edition 2024), [Axum](https://github.com/tokio-rs/axum) 0.8.9, [Tokio](https://tokio.rs/) 1.54.1, [SQLx](https://github.com/launchbadge/sqlx) 0.9.1, PostgreSQL 18, [dotenvy](https://github.com/allan2/dotenvy) 0.15.7, [Serde](https://serde.rs/) 1.0.229, [thiserror](https://github.com/dtolnay/thiserror) 2.0.21 and [validator](https://github.com/Keats/validator) 0.21.0.

![Demo: cargo run starts the server, curl creates Alice (201), a second POST with ALICE@EXAMPLE.COM gets a 409 email_taken from the database's UNIQUE constraint, the server is restarted and GET /users/1 still returns Alice, GET /users/99 returns 404, and psql shows the row in the users table](docs/demo.gif)

**Full video tutorial on YouTube (18 min):** https://youtu.be/XPcp-o8gmqs

---

## What it does

Same five routes and the same JSON as lesson 4, now backed by PostgreSQL:

| Method | Route | Query | Success |
|---|---|---|---|
| `POST` | `/users` | `INSERT ... RETURNING` with `fetch_one` | `201 Created` + the new user |
| `GET` | `/users` | `SELECT ... WHERE $1::text IS NULL OR name ILIKE ...` with `fetch_all` (optional `?name=` filter) | `200 OK` + list |
| `GET` | `/users/{id}` | `SELECT ... WHERE id = $1` with `fetch_optional` | `200 OK` + user |
| `PUT` | `/users/{id}` | `UPDATE ... RETURNING` with `fetch_optional` | `200 OK` + updated user |
| `DELETE` | `/users/{id}` | `DELETE` with `execute` and `rows_affected()` | `204 No Content` |

## How it works: from memory to a database

The table is created by a migration ([`migrations/`](migrations)). The unique email rule now lives in the database:

```sql
CREATE TABLE users (
    id BIGINT GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    name TEXT NOT NULL,
    email TEXT NOT NULL UNIQUE
);
```

On startup the server reads `DATABASE_URL` from `.env`, opens a connection pool and applies any pending migration:

```rust
dotenvy::dotenv().ok();
let database_url = std::env::var("DATABASE_URL").expect("DATABASE_URL must be set");
let pool = PgPoolOptions::new()
    .max_connections(5)
    .connect(&database_url)
    .await
    .expect("failed to connect to Postgres");
sqlx::migrate!()
    .run(&pool)
    .await
    .expect("failed to run migrations");
```

### A pool instead of a lock

The state used to be `Arc<RwLock<Db>>`. Now it is just the pool:

```rust
type AppState = PgPool;
```

A `PgPool` is already cheap to clone (there is an `Arc` inside), so the `Arc` and the `RwLock` go away. The database handles concurrent access.

### SQL the compiler checks

`query_as!` connects to the database during `cargo build` and checks the SQL, the columns and the types against the `User` struct:

```rust
async fn get_user(
    State(pool): State<AppState>,
    AppPath(id): AppPath<i64>,
) -> Result<Json<User>, ApiError> {
    let user = sqlx::query_as!(
        User,
        r#"SELECT id, name AS "name: UserName", email AS "email: Email" FROM users WHERE id = $1"#,
        id
    )
    .fetch_optional(&pool)
    .await?;
    user.map(Json).ok_or(ApiError::UserNotFound(id))
}
```

- `fetch_optional` returns an `Option<User>`: `None` becomes the `404`.
- Postgres has no unsigned integers, so `id` is now an `i64` (`BIGINT`) instead of a `u64`.
- `name AS "name: UserName"` tells SQLx to read the column straight into the newtype. The newtypes from lesson 4 derive `sqlx::Type` with `#[sqlx(transparent)]`, which means "in the database, treat it exactly like the string inside" (see [`src/domain.rs`](src/domain.rs)).
- In `list_users`, an `Option<String>` parameter becomes `NULL` when there is no filter, so `$1::text IS NULL` returns every user.

## Error handling

`ApiError` gets one new variant. Any `sqlx::Error` converts into it with `?`, and the client gets a plain `500` without the database details (they are logged on the server):

```rust
#[error("internal server error")]
Database(#[from] sqlx::Error),
```

The duplicate email check no longer reads all users before inserting. The `UNIQUE` constraint does the check, with no race between check and insert, and a unique violation becomes the same `409` as before:

```rust
fn email_taken(err: sqlx::Error, email: &Email) -> ApiError {
    match err {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::EmailTaken(email.as_str().to_string())
        }
        other => ApiError::from(other),
    }
}
```

Emails are already trimmed and lowercased by the `Email` type (lesson 4), so a plain `UNIQUE` is enough: `ALICE@EXAMPLE.COM` hits the same row as `alice@example.com`.

| Status | `code` | When |
|---|---|---|
| `422 Unprocessable Entity` | `validation_failed` | `name` or `email` breaks a rule (same as lesson 4) |
| `409 Conflict` | `email_taken` | the database rejects a duplicate email (unique violation) |
| `404 Not Found` | `user_not_found` | no row with that id (`fetch_optional` returned `None`, or `DELETE` affected 0 rows) |
| `400 Bad Request` | `invalid_path` | the id in the path is not a number |
| `400` / `415` / `422` | `invalid_json` | malformed JSON, missing `content-type`, or a missing field |
| `500 Internal Server Error` | `internal_error` | any other database error |

## The compiler errors from the video

1. **`error returned from database: column "emal" does not exist`.** A typo in a column name inside `query_as!` fails `cargo build` with the error from Postgres itself: the query was checked against the real table.
2. **`the trait bound UserName: From<String> is not satisfied` (E0277).** After fixing the typo, the types didn't match: the table returns `BIGINT` and `TEXT`, and `User` asked for `u64`, `UserName` and `Email`. The fix was `i64` for the id, `#[sqlx(transparent)]` on the newtypes and the `AS "name: UserName"` annotations.

## Trying it with curl

```bash
curl -i -X POST localhost:8080/users -H 'content-type: application/json' -d '{"name":"Alice","email":"alice@example.com"}'   # 201
curl -i -X POST localhost:8080/users -H 'content-type: application/json' -d '{"name":"Alice","email":"ALICE@EXAMPLE.COM"}'   # 409 email_taken
```

Stop the server (Ctrl+C), run `cargo run` again, then:

```bash
curl -i localhost:8080/users/1      # 200, Alice is still there
curl -i localhost:8080/users/99     # 404 user_not_found
docker compose exec db psql -U wallet -c 'SELECT * FROM users'
```

## Project structure

```
05-postgres-sqlx/
├── .env                  # DATABASE_URL for the local Postgres (development credentials only)
├── docker-compose.yml    # PostgreSQL 18 with a named volume
├── Cargo.toml            # axum, dotenvy, serde, sqlx (postgres, macros, migrate), thiserror, tokio, validator
├── Cargo.lock
├── docs/demo.gif
├── migrations/
│   └── 20261120171858_create_users.sql   # users table, UNIQUE email
└── src/
    ├── domain.rs         # UserName and Email newtypes, now with sqlx::Type and #[sqlx(transparent)]
    └── main.rs           # PgPool state, migrations on startup, ApiError (with Database), handlers with query_as! / query!
```

## Running it

Requirements: Rust 1.85 or newer (edition 2024), Docker with Compose, the SQLx CLI 0.9 and `curl` to test.

```bash
cargo install sqlx-cli
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/05-postgres-sqlx
docker compose up -d
sqlx migrate run
cargo run
```

The order matters: the query macros check the SQL against the database while compiling, so Postgres must be running and the `users` table must exist before `cargo build` / `cargo run`. After that, the server also applies new migrations on its own at startup (`sqlx::migrate!()`).

The API listens on `http://localhost:8080`. The `.env` file holds the local development connection string (`postgres://wallet:wallet@localhost:5432/wallet`), matching `docker-compose.yml`. Don't reuse these credentials outside your machine.

Dependencies:

```toml
[dependencies]
axum = { version = "0.8.9", features = ["macros"] }
dotenvy = "0.15.7"
serde = { version = "1.0.229", features = ["derive"] }
sqlx = { version = "0.9.1", features = ["runtime-tokio", "tls-rustls", "postgres", "macros", "migrate"] }
thiserror = "2.0.21"
tokio = { version = "1.54.1", features = ["full"] }
validator = "0.21.0"
```

To stop Postgres: `docker compose down` (add `-v` to also delete the data).

## Important note

This project is for learning. So far it only has users: no authentication, accounts or balances yet.

---

Made by Fernando Santos · [YouTube tutorial](https://youtu.be/XPcp-o8gmqs) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
