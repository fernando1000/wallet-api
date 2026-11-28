use sqlx::postgres::PgPoolOptions;
use tokio::net::TcpListener;
use wallet_api::{config::Config, repository::PgUserRepository, router, telemetry};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();
    let config = Config::from_env().unwrap_or_else(|err| {
        eprintln!("configuration error: {err}");
        std::process::exit(1);
    });
    telemetry::init(config.env);
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await
        .expect("failed to connect to Postgres");
    sqlx::migrate!()
        .run(&pool)
        .await
        .expect("failed to run migrations");
    let listener = TcpListener::bind(("0.0.0.0", config.port)).await.unwrap();
    let repo = PgUserRepository::new(pool);
    tracing::info!(port = config.port, env = ?config.env, "starting server");
    axum::serve(listener, router(repo)).await.unwrap();
}
