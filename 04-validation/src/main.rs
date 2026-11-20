mod domain;

use std::collections::HashMap;
use std::sync::{Arc, PoisonError, RwLock};

use axum::{
    Json, Router, debug_handler,
    extract::{
        FromRequest, FromRequestParts, Path, Query, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tokio::net::TcpListener;

use domain::{Email, UserName};

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: UserName,
    email: Email,
}

#[derive(Deserialize)]
struct UserInput {
    name: String,
    email: String,
}

struct NewUser {
    name: UserName,
    email: Email,
}

impl TryFrom<UserInput> for NewUser {
    type Error = ApiError;

    fn try_from(input: UserInput) -> Result<Self, Self::Error> {
        let mut errors = Vec::new();
        let name = UserName::try_from(input.name)
            .map_err(|message| {
                errors.push(FieldError {
                    field: "name",
                    message,
                })
            })
            .ok();
        let email = Email::try_from(input.email)
            .map_err(|message| {
                errors.push(FieldError {
                    field: "email",
                    message,
                })
            })
            .ok();
        match (name, email) {
            (Some(name), Some(email)) => Ok(NewUser { name, email }),
            _ => Err(ApiError::Validation(errors)),
        }
    }
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

#[derive(Debug, Serialize)]
struct FieldError {
    field: &'static str,
    message: String,
}

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("user {0} not found")]
    UserNotFound(u64),
    #[error("email {0} is already taken")]
    EmailTaken(String),
    #[error("invalid JSON body: {0}")]
    InvalidJson(#[from] JsonRejection),
    #[error("invalid path: {0}")]
    InvalidPath(#[from] PathRejection),
    #[error("invalid request data")]
    Validation(Vec<FieldError>),
    #[error("internal server error")]
    Internal,
}

#[derive(Serialize)]
struct ErrorBody {
    code: &'static str,
    message: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    fields: Vec<FieldError>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::UserNotFound(_) => (StatusCode::NOT_FOUND, "user_not_found"),
            ApiError::EmailTaken(_) => (StatusCode::CONFLICT, "email_taken"),
            ApiError::InvalidJson(rejection) => (rejection.status(), "invalid_json"),
            ApiError::InvalidPath(rejection) => (rejection.status(), "invalid_path"),
            ApiError::Validation(_) => (StatusCode::UNPROCESSABLE_ENTITY, "validation_failed"),
            ApiError::Internal => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        let message = self.to_string();
        let fields = match self {
            ApiError::Validation(fields) => fields,
            _ => Vec::new(),
        };
        let body = ErrorBody {
            code,
            message,
            fields,
        };
        (status, Json(body)).into_response()
    }
}

impl<T> From<PoisonError<T>> for ApiError {
    fn from(_: PoisonError<T>) -> Self {
        ApiError::Internal
    }
}

#[derive(FromRequest)]
#[from_request(via(Json), rejection(ApiError))]
struct AppJson<T>(T);

#[derive(FromRequestParts)]
#[from_request(via(Path), rejection(ApiError))]
struct AppPath<T>(T);

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
    AppJson(input): AppJson<UserInput>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let new_user = NewUser::try_from(input)?;
    let user = {
        let mut db = state.write()?;
        if db.users.values().any(|user| user.email == new_user.email) {
            return Err(ApiError::EmailTaken(new_user.email.as_str().to_string()));
        }
        db.next_id += 1;
        let user = User {
            id: db.next_id,
            name: new_user.name,
            email: new_user.email,
        };
        db.users.insert(user.id, user.clone());
        user
    };
    Ok((StatusCode::CREATED, Json(user)))
}

async fn list_users(
    State(state): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<User>>, ApiError> {
    let filter = params.name.map(|name| name.to_lowercase());
    let db = state.read()?;
    let mut users: Vec<User> = db
        .users
        .values()
        .filter(|user| {
            filter
                .as_ref()
                .is_none_or(|name| user.name.as_str().to_lowercase().contains(name))
        })
        .cloned()
        .collect();
    users.sort_by_key(|user| user.id);
    Ok(Json(users))
}

async fn get_user(
    State(state): State<AppState>,
    AppPath(id): AppPath<u64>,
) -> Result<Json<User>, ApiError> {
    let db = state.read()?;
    db.users
        .get(&id)
        .cloned()
        .map(Json)
        .ok_or(ApiError::UserNotFound(id))
}

#[debug_handler]
async fn update_user(
    State(state): State<AppState>,
    AppPath(id): AppPath<u64>,
    AppJson(input): AppJson<UserInput>,
) -> Result<Json<User>, ApiError> {
    let new_user = NewUser::try_from(input)?;
    let mut db = state.write()?;
    if db
        .users
        .values()
        .any(|user| user.id != id && user.email == new_user.email)
    {
        return Err(ApiError::EmailTaken(new_user.email.as_str().to_string()));
    }
    let user = db.users.get_mut(&id).ok_or(ApiError::UserNotFound(id))?;
    user.name = new_user.name;
    user.email = new_user.email;
    Ok(Json(user.clone()))
}

async fn delete_user(
    State(state): State<AppState>,
    AppPath(id): AppPath<u64>,
) -> Result<StatusCode, ApiError> {
    if state.write()?.users.remove(&id).is_none() {
        return Err(ApiError::UserNotFound(id));
    }
    Ok(StatusCode::NO_CONTENT)
}
