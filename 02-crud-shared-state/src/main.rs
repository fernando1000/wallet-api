use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use axum::{
    Json, Router, debug_handler,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct UserInput {
    name: String,
    email: String,
}

#[derive(Deserialize)]
struct ListParams {
    name: Option<String>,
}

#[derive(Default)]
struct Db {
    users: HashMap<u64, User>,
    next_id: u64,
}

type AppState = Arc<RwLock<Db>>;

#[tokio::main]
async fn main() {
    let state = AppState::default();
    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    let router = Router::new()
        .route("/users", post(create_user).get(list_users))
        .route(
            "/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .with_state(state);
    println!("Application running on port 8080...");
    axum::serve(listener, router).await.unwrap();
}

async fn create_user(
    State(state): State<AppState>,
    Json(input): Json<UserInput>,
) -> (StatusCode, Json<User>) {
    let user = {
        let mut db = state.write().unwrap();
        db.next_id += 1;
        let user = User {
            id: db.next_id,
            name: input.name,
            email: input.email,
        };
        db.users.insert(user.id, user.clone());
        user
    };
    (StatusCode::CREATED, Json(user))
}

async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Json<Vec<User>> {
    let filter = params.name.map(|name| name.to_lowercase());
    let db = state.read().unwrap();
    let mut users: Vec<User> = db
        .users
        .values()
        .filter(|user| {
            filter
                .as_ref()
                .is_none_or(|name| user.name.to_lowercase().contains(name))
        })
        .cloned()
        .collect();
    users.sort_by_key(|user| user.id);
    Json(users)
}

async fn get_user(
    State(state): State<AppState>,
    Path(id): Path<u64>,
) -> Result<Json<User>, StatusCode> {
    let db = state.read().unwrap();
    db.users
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(StatusCode::NOT_FOUND)
}

#[debug_handler]
async fn update_user(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    Json(input): Json<UserInput>,
) -> Result<Json<User>, StatusCode> {
    let mut db = state.write().unwrap();
    let user = db.users.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    user.name = input.name;
    user.email = input.email;
    Ok(Json(user.clone()))
}

async fn delete_user(State(state): State<AppState>, Path(id): Path<u64>) -> StatusCode {
    if state.write().unwrap().users.remove(&id).is_some() {
        StatusCode::NO_CONTENT
    } else {
        StatusCode::NOT_FOUND
    }
}
