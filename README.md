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

Each lesson is also tagged (`lesson-01`, `lesson-02`, ...), so you can check out the repository as it was after that video.

## Running a lesson

Requirements: Rust 1.85 or newer (edition 2024).

```bash
git clone https://github.com/fernando1000/wallet-api.git
cd wallet-api/04-validation
cargo run
```

The API listens on `http://localhost:8080`. The README in each folder lists the requests to try.

## Stack

[Axum](https://github.com/tokio-rs/axum) 0.8 · [Tokio](https://tokio.rs/) 1 · [Serde](https://serde.rs/) 1 · [thiserror](https://github.com/dtolnay/thiserror) 2 (from lesson 3) · [validator](https://github.com/Keats/validator) 0.21 (from lesson 4)

---

Made by Fernando Santos · [YouTube](https://youtu.be/o4VHyqKAC1U) · [LinkedIn](https://www.linkedin.com/in/fernando-blockchain)
