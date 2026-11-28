# Lesson 8: Config and Structured Logging for a Rust Axum API

Lesson 7 ended with a well-tested API that still had a fixed port, an `expect` on `DATABASE_URL` and `println!` / `eprintln!` lines with no context. When a database error showed up, nothing said which request caused it. This lesson gets the API ready to operate. A typed `Config` is read once from the environment and validated at startup, so a missing or invalid value stops the server right away with a clear message. Logs become structured events from [tracing](https://github.com/tokio-rs/tracing): readable text in development, JSON in production. [tower-http](https://github.com/tower-rs/tower-http) gives every request a UUID in the `x-request-id` header and a span that ties each log line to that request. Built with Rust (edition 2024), [Axum](https://github.com/tokio-rs/axum) 0.8.9, [Tokio](https://tokio.rs/) 1.54.1, [SQLx](https://github.com/launchbadge/sqlx) 0.9.1, PostgreSQL 18, tracing 0.1.44, tracing-subscriber 0.3.23, tower-http 0.7.1, [tower](https://github.com/tower-rs/tower) 0.5.3, [dotenvy](https://github.com/allan2/dotenvy) 0.15.7, [Serde](https://serde.rs/) 1.0.229, [thiserror](https://github.com/dtolnay/thiserror) 2.0.21 and [validator](https://github.com/Keats/validator) 0.21.0. The tests also use testcontainers-modules 0.15.0 and [serde_json](https://github.com/serde-rs/json) 1.0.154.

![Demo: the server restarts with APP_ENV=production and logs JSON. A curl to /users/99 returns 404 with an x-request-id header, and the same request_id appears in every JSON log line for that request (started processing, finished with status 404, end of stream)](docs/demo.gif)

**Full video tutorial on YouTube (18 min):** https://youtu.be/M8NT8bw6oCg

---

## What it does

The routes, JSON bodies and status codes are the same as in lesson 7. What changes is how the server starts and what it tells you while it runs:

| | Before (lesson 7) | Now |
|---|---|---|
| Configuration | port `8080` hard-coded, `expect` on `DATABASE_URL` | `Config { database_url, port, env }`, read once and validated at startup |
| Bad config | a panic message | `configuration error: invalid value for APP_ENV: "prod"` and exit code `1` |
| Logs | `println!` / `eprintln!` | `tracing` events with fields (`port`, `env`, `user_id`, `error`) |
| Log format | plain text | readable text in development, JSON in production |
| Request id | none | a UUID in `x-request-id` on every response, kept if the client sends one, and in every log line of that request |

Environment variables:

| Variable | Default | Meaning |
|---|---|---|
| `DATABASE_URL` | none (required) | Postgres connection string |
| `PORT` | `8080` | port the server listens on, must be a valid `u16` |
| `APP_ENV` | `development` | `development` or `production`, anything else is rejected |
| `RUST_LOG` | `wallet_api=debug,tower_http=debug,info` | log filter, read by `EnvFilter` |

## How it works: config once, logs with context

### A typed config, validated at startup

[`src/config.rs`](src/config.rs) turns strings from the environment into types. The environment is an `enum`, so a typo can't pass as a third environment:

```rust
pub fn from_vars<F>(get: F) -> Result<Self, ConfigError>
where
    F: Fn(&str) -> Option<String>,
{
    let database_url = get("DATABASE_URL").ok_or(ConfigError::Missing("DATABASE_URL"))?;
    let port = match get("PORT") {
        None => 8080,
        Some(value) => value.parse().map_err(|_| ConfigError::Invalid {
            name: "PORT",
            value,
        })?,
    };
    let env = match get("APP_ENV").as_deref() {
        None | Some("development") => AppEnv::Development,
        Some("production") => AppEnv::Production,
        Some(other) => {
            return Err(ConfigError::Invalid {
                name: "APP_ENV",
                value: other.to_string(),
            });
        }
    };
    Ok(Config { database_url, port, env })
}

pub fn from_env() -> Result<Self, ConfigError> {
    Self::from_vars(|name| std::env::var(name).ok())
}
```

`from_vars` doesn't know where the values come from. In the server they come from `std::env::var`. In the tests they come from a `HashMap`, so the tests never touch the process environment. [`src/main.rs`](src/main.rs) reads the config once, as its first step, and stops if it fails:

```rust
let config = Config::from_env().unwrap_or_else(|err| {
    eprintln!("configuration error: {err}");
    std::process::exit(1);
});
telemetry::init(config.env);
```

After that the config never changes: no `mut`, no global, only borrowed where it's needed. It's only used in `main`, so it doesn't need a `OnceLock` either. The `Config` is never logged as a whole, because `DATABASE_URL` contains the password.

### Structured logs

[`src/telemetry.rs`](src/telemetry.rs) sets up the subscriber with `EnvFilter` and picks the format from the environment:

```rust
match env {
    AppEnv::Development => builder.init(),
    AppEnv::Production => builder.json().init(),
}
```

Events carry fields instead of text glued into a message: `tracing::info!(port = config.port, env = ?config.env, "starting server")`, `tracing::info!(user_id = user.id, "user created")`, and `tracing::error!(error = %err, "database error")` in `ApiError`.

### A request id in every log line

The layers live inside `router()` in [`src/lib.rs`](src/lib.rs), so the integration tests get them too:

```rust
.with_state(repo)
.layer(
    ServiceBuilder::new()
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .layer(TraceLayer::new_for_http().make_span_with(request_span))
        .layer(PropagateRequestIdLayer::x_request_id()),
)
```

`SetRequestIdLayer` adds a UUID to the request (or keeps the one the client sent), `TraceLayer` opens one span per request with `method`, `uri` and `request_id`, and `PropagateRequestIdLayer` copies the id into the response header. Every event logged while handling the request, including the database error, is inside that span.

## The errors from the video

1. **``error[E0133]: call to unsafe function `set_var` is unsafe and requires unsafe block``.** The first config test changed the process environment with `std::env::set_var`. Since edition 2024 that is `unsafe`: the environment is global and tests run in parallel threads. Fixed with `from_vars`, so the tests pass a `HashMap` instead.
2. **``error[E0599]: no method named `json` found for struct `SubscriberBuilder` ``.** JSON output is behind a crate feature. Fixed with `cargo add tracing-subscriber --features json`.
3. **No `x-request-id` in the response.** With three chained `Router::layer` calls, the last one added is the outermost, and it sees the request first. So the layers ran in the opposite order from how they read: the trace span opened before any id existed, and `PropagateRequestIdLayer` looked for an id that wasn't there yet. Fixed with tower's `ServiceBuilder`, which applies layers top to bottom.

## Tests

Unit tests for the config ([`src/config.rs`](src/config.rs)), with no environment changes:

| Test | What it proves |
|---|---|
| `typo_in_app_env_is_rejected` | `APP_ENV=prod` fails with `invalid value for APP_ENV: "prod"` |
| `missing_database_url_is_an_error` | no `DATABASE_URL` returns `ConfigError::Missing("DATABASE_URL")` |
| `defaults_to_port_8080_in_development` | with only `DATABASE_URL` set, the port is `8080` and the env is `Development` |

New integration test ([`tests/api.rs`](tests/api.rs)):

| Test | What it proves |
|---|---|
| `responses_have_a_request_id` | every response has an `x-request-id` header, and a request sent with `x-request-id: test-123` gets the same id back |

The 3 handler unit tests in `src/lib.rs` and the 4 integration tests from lesson 7 are unchanged. One `cargo test` runs 6 unit tests and 5 integration tests.

## Project structure

```
08-config-logging/
├── .env                  # DATABASE_URL for the local Postgres (development credentials only)
├── docker-compose.yml    # PostgreSQL 18 with a named volume
├── Cargo.toml            # + tracing, tracing-subscriber (env-filter, json), tower-http (trace, request-id), tower
├── Cargo.lock
├── docs/demo.gif
├── migrations/
│   └── 20261120171858_create_users.sql   # users table, UNIQUE email
├── src/
│   ├── config.rs         # Config, AppEnv, ConfigError, from_vars / from_env, config tests
│   ├── telemetry.rs      # tracing subscriber: EnvFilter, text or JSON
│   ├── domain.rs         # UserName and Email newtypes
│   ├── repository.rs     # UserRepository trait, RepoError, PgUserRepository, InMemoryUserRepository (tests only)
│   ├── lib.rs            # ApiError, pub router with the request id and trace layers, handlers, unit tests
│   └── main.rs           # main(): config, telemetry, pool, migrations, server
└── tests/
    └── api.rs            # integration tests: spawn_app(), request helpers, 5 tests
```

## Running it

Requirements: Rust 1.85 or newer (edition 2024), Docker with Compose and the SQLx CLI 0.9.

```bash
cargo install sqlx-cli
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/08-config-logging
docker compose up -d
sqlx migrate run
cargo test
```

Postgres from `docker-compose.yml` must be running and the `users` table must exist before `cargo build` or `cargo test`, because the SQLx query macros check the SQL against the database while compiling. The integration tests start their own `postgres:18` container, so Docker must be running.

Run the server in development (readable logs) and send a request:

```bash
cargo run
curl -i localhost:8080/users/1
curl -i -H 'x-request-id: demo-123' localhost:8080/users/1
```

Run it in production mode (JSON logs):

```bash
APP_ENV=production cargo run
curl -i localhost:8080/users/99
```

See the fail fast at startup:

```bash
APP_ENV=prod cargo run
# configuration error: invalid value for APP_ENV: "prod"
```

Change the log level without recompiling, for example `RUST_LOG=info cargo run`.

The `.env` file holds the local development connection string (`postgres://wallet:wallet@localhost:5432/wallet`), matching `docker-compose.yml`. Don't reuse these credentials outside your machine.

To stop Postgres: `docker compose down` (add `-v` to also delete the data).

## Important note

This project is for learning. So far it only has users: no authentication, accounts or balances yet.

---

Made by Fernando Santos · [YouTube tutorial](https://youtu.be/M8NT8bw6oCg) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
