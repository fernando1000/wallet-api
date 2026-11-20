mod domain;

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
use sqlx::{PgPool, postgres::PgPoolOptions};
use tokio::net::TcpListener;

use domain::{Email, UserName};

#[derive(Clone, Serialize)]
struct User {
    id: i64,
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

type AppState = PgPool;

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
            eprintln!("database error: {err}");
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

#[tokio::main]
async fn main() {
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
    let listener = TcpListener::bind("0.0.0.0:8080").await.unwrap();
    let router = Router::new()
        .route("/users", post(create_user).get(list_users))
        .route(
            "/users/{id}",
            get(get_user).put(update_user).delete(delete_user),
        )
        .with_state(pool);
    println!("Application running on port 8080...");
    axum::serve(listener, router).await.unwrap();
}

async fn create_user(
    State(pool): State<AppState>,
    AppJson(input): AppJson<UserInput>,
) -> Result<(StatusCode, Json<User>), ApiError> {
    let new_user = NewUser::try_from(input)?;
    let user = sqlx::query_as!(
        User,
        r#"INSERT INTO users (name, email) VALUES ($1, $2)
        RETURNING id, name AS "name: UserName", email AS "email: Email""#,
        new_user.name.as_str(),
        new_user.email.as_str()
    )
    .fetch_one(&pool)
    .await
    .map_err(|err| email_taken(err, &new_user.email))?;
    Ok((StatusCode::CREATED, Json(user)))
}

fn email_taken(err: sqlx::Error, email: &Email) -> ApiError {
    match err {
        sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
            ApiError::EmailTaken(email.as_str().to_string())
        }
        other => ApiError::from(other),
    }
}

async fn list_users(
    State(pool): State<AppState>,
    Query(params): Query<ListParams>,
) -> Result<Json<Vec<User>>, ApiError> {
    let users = sqlx::query_as!(
        User,
        r#"SELECT id, name AS "name: UserName", email AS "email: Email" FROM users
        WHERE $1::text IS NULL OR name ILIKE '%' || $1 || '%'
        ORDER BY id"#,
        params.name
    )
    .fetch_all(&pool)
    .await?;
    Ok(Json(users))
}

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

#[debug_handler]
async fn update_user(
    State(pool): State<AppState>,
    AppPath(id): AppPath<i64>,
    AppJson(input): AppJson<UserInput>,
) -> Result<Json<User>, ApiError> {
    let new_user = NewUser::try_from(input)?;
    let user = sqlx::query_as!(
        User,
        r#"UPDATE users SET name = $1, email = $2 WHERE id = $3
        RETURNING id, name AS "name: UserName", email AS "email: Email""#,
        new_user.name.as_str(),
        new_user.email.as_str(),
        id
    )
    .fetch_optional(&pool)
    .await
    .map_err(|err| email_taken(err, &new_user.email))?;
    user.map(Json).ok_or(ApiError::UserNotFound(id))
}

async fn delete_user(
    State(pool): State<AppState>,
    AppPath(id): AppPath<i64>,
) -> Result<StatusCode, ApiError> {
    let result = sqlx::query!("DELETE FROM users WHERE id = $1", id)
        .execute(&pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(ApiError::UserNotFound(id));
    }
    Ok(StatusCode::NO_CONTENT)
}
