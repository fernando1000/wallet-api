use axum::{
    Router,
    body::Body,
    http::{Request, StatusCode, header::CONTENT_TYPE},
    response::Response,
};
use serde_json::{Value, json};
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;
use testcontainers_modules::{
    postgres::Postgres,
    testcontainers::{ContainerAsync, ImageExt, runners::AsyncRunner},
};
use tower::ServiceExt;
use wallet_api::{repository::PgUserRepository, router};

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

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn post_json(uri: &str, body: Value) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header(CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

async fn read_json(response: Response) -> Value {
    let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

#[tokio::test]
async fn missing_user_returns_404() {
    let (app, _container) = spawn_app().await;
    let response = app.oneshot(get("/users/99")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    let body = read_json(response).await;
    assert_eq!(body["code"], "user_not_found");
}

#[tokio::test]
async fn create_and_get_user() {
    let (app, _container) = spawn_app().await;
    let alice = json!({ "name": "Alice", "email": "alice@example.com" });
    let response = app
        .clone()
        .oneshot(post_json("/users", alice))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let body = read_json(response).await;
    assert_eq!(body["id"], 1);
    let response = app.oneshot(get("/users/1")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(read_json(response).await["name"], "Alice");
}

#[tokio::test]
async fn duplicate_email_returns_409() {
    let (app, _container) = spawn_app().await;
    let alice = json!({ "name": "Alice", "email": "alice@example.com" });
    let response = app
        .clone()
        .oneshot(post_json("/users", alice))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let again = json!({ "name": "Alice", "email": "ALICE@EXAMPLE.COM" });
    let response = app.oneshot(post_json("/users", again)).await.unwrap();
    assert_eq!(response.status(), StatusCode::CONFLICT);
    assert_eq!(read_json(response).await["code"], "email_taken");
}

#[tokio::test]
async fn invalid_email_returns_422() {
    let (app, _container) = spawn_app().await;
    let bob = json!({ "name": "Bob", "email": "not-an-email" });
    let response = app.oneshot(post_json("/users", bob)).await.unwrap();
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    let body = read_json(response).await;
    assert_eq!(body["code"], "validation_failed");
    assert_eq!(body["fields"][0]["field"], "email");
}
