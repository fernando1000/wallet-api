pub mod config;
mod domain;
pub mod repository;
pub mod telemetry;

use axum::{
    Json, Router,
    body::Body,
    extract::{
        FromRequest, FromRequestParts, Path, Query, State,
        rejection::{JsonRejection, PathRejection},
    },
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use serde::{Deserialize, Serialize};
use tower::ServiceBuilder;
use tower_http::{
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    trace::TraceLayer,
};
use tracing::Span;

use domain::{Email, UserName};
use repository::{RepoError, UserRepository};

#[derive(Clone, Serialize)]
pub struct User {
    id: i64,
    name: UserName,
    email: Email,
}

#[derive(Deserialize)]
struct UserInput {
    name: String,
    email: String,
}

pub struct NewUser {
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

#[derive(Debug, Serialize)]
struct FieldError {
    field: &'static str,
    message: String,
}

#[derive(Debug, thiserror::Error)]
enum ApiError {
    #[error("user {0} not found")]
    UserNotFound(i64),
    #[error("email {0} is already taken")]
    EmailTaken(String),
    #[error("invalid JSON body: {0}")]
    InvalidJson(#[from] JsonRejection),
    #[error("invalid path: {0}")]
    InvalidPath(#[from] PathRejection),
    #[error("invalid request data")]
    Validation(Vec<FieldError>),
    #[error("internal server error")]
    Database(#[from] sqlx::Error),
}

impl From<RepoError> for ApiError {
    fn from(err: RepoError) -> Self {
        match err {
            RepoError::EmailTaken(email) => ApiError::EmailTaken(email),
            RepoError::Database(err) => ApiError::Database(err),
        }
    }
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
            ApiError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if let ApiError::Database(err) = &self {
            tracing::error!(error = %err, "database error");
        }
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

#[derive(FromRequest)]
#[from_request(via(Json), rejection(ApiError))]
struct AppJson<T>(T);

#[derive(FromRequestParts)]
#[from_request(via(Path), rejection(ApiError))]
struct AppPath<T>(T);

pub fn router<R: UserRepository>(repo: R) -> Router {
    Router::new()
        .route("/users", post(create_user::<R>).get(list_users::<R>))
        .route(
            "/users/{id}",
            get(get_user::<R>)
                .put(update_user::<R>)
                .delete(delete_user::<R>),
        )
        .with_state(repo)
        .layer(
            ServiceBuilder::new()
                .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
                .layer(TraceLayer::new_for_http().make_span_with(request_span))
                .layer(PropagateRequestIdLayer::x_request_id()),
        )
}

fn request_span(request: &Request<Body>) -> Span {
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();
    tracing::info_span!(
        "request",
        method = %request.method(),
        uri = %request.uri(),
        request_id
    )
}

async fn create_user<R: UserRepository>(
    State(repo): State<R>,
    AppJson(input): AppJson<UserInput>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let new_user = NewUser::try_from(input)?;
    let user = repo.create(new_user).await?;
    tracing::info!(user_id = user.id, "user created");
    Ok((StatusCode::CREATED, Json(user)))
}

async fn list_users<R: UserRepository>(
    State(repo): State<R>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<User>>, ApiError> {
    let users = repo.list(params.name).await?;
    Ok(Json(users))
}

async fn get_user<R: UserRepository>(
    State(repo): State<R>,
    AppPath(id): AppPath<i64>,
) -> Result<Json<User>, ApiError> {
    let user = repo.find(id).await?;
    user.map(Json).ok_or(ApiError::UserNotFound(id))
}

async fn update_user<R: UserRepository>(
    State(repo): State<R>,
    AppPath(id): AppPath<i64>,
    AppJson(input): AppJson<UserInput>,
) -> Result<Json<User>, ApiError> {
    let new_user = NewUser::try_from(input)?;
    let user = repo.update(id, new_user).await?;
    user.map(Json).ok_or(ApiError::UserNotFound(id))
}

async fn delete_user<R: UserRepository>(
    State(repo): State<R>,
    AppPath(id): AppPath<i64>,
) -> Result<StatusCode, ApiError> {
    if !repo.delete(id).await? {
        return Err(ApiError::UserNotFound(id));
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;
    use repository::in_memory::InMemoryUserRepository;

    fn input(name: &str, email: &str) -> AppJson<UserInput> {
        AppJson(UserInput {
            name: name.to_string(),
            email: email.to_string(),
        })
    }

    #[tokio::test]
    async fn create_returns_201_with_id() {
        let repo = InMemoryUserRepository::default();
        let (status, Json(user)) = create_user(State(repo), input("Alice", "alice@example.com"))
            .await
            .unwrap();
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(user.id, 1);
    }

    #[tokio::test]
    async fn duplicate_email_is_taken() {
        let repo = InMemoryUserRepository::default();
        let first = create_user(State(repo.clone()), input("Alice", "alice@example.com")).await;
        assert!(first.is_ok());
        let second = create_user(State(repo), input("Alice", "ALICE@EXAMPLE.COM")).await;
        assert!(matches!(second, Err(ApiError::EmailTaken(_))));
    }

    #[tokio::test]
    async fn missing_user_is_not_found() {
        let repo = InMemoryUserRepository::default();
        let result = get_user(State(repo), AppPath(99)).await;
        assert!(matches!(result, Err(ApiError::UserNotFound(99))));
    }
}
