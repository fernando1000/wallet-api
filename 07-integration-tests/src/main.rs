use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use wallet_api::{repository::PgUserRepository, router};

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
    let repo = PgUserRepository::new(pool);
    println!("Application running on port 8080...");
    axum::serve(listener, router(repo)).await.unwrap();
}
