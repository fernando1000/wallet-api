use tracing_subscriber::EnvFilter;

use crate::config::AppEnv;

pub fn init(env: AppEnv) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("wallet_api=debug,tower_http=debug,info"));
    let builder = tracing_subscriber::fmt().with_env_filter(filter);
    match env {
        AppEnv::Development => builder.init(),
        AppEnv::Production => builder.json().init(),
    }
}
