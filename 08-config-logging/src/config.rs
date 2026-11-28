#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AppEnv {
    Development,
    Production,
}

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum ConfigError {
    #[error("missing environment variable {0}")]
    Missing(&'static str),
    #[error("invalid value for {name}: {value:?}")]
    Invalid { name: &'static str, value: String },
}

#[derive(Debug)]
pub struct Config {
    pub database_url: String,
    pub port: u16,
    pub env: AppEnv,
}

impl Config {
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
        Ok(Config {
            database_url,
            port,
            env,
        })
    }

    pub fn from_env() -> Result<Self, ConfigError> {
        Self::from_vars(|name| std::env::var(name).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn config_from(vars: &[(&str, &str)]) -> Result<Config, ConfigError> {
        let map: HashMap<String, String> = vars
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();
        Config::from_vars(|name| map.get(name).cloned())
    }

    #[test]
    fn typo_in_app_env_is_rejected() {
        let vars = [
            ("DATABASE_URL", "postgres://localhost/test"),
            ("APP_ENV", "prod"),
        ];
        let err = config_from(&vars).unwrap_err();
        assert_eq!(err.to_string(), r#"invalid value for APP_ENV: "prod""#);
    }

    #[test]
    fn missing_database_url_is_an_error() {
        let err = config_from(&[]).unwrap_err();
        assert_eq!(err, ConfigError::Missing("DATABASE_URL"));
    }

    #[test]
    fn defaults_to_port_8080_in_development() {
        let config = config_from(&[("DATABASE_URL", "postgres://localhost/test")]).unwrap();
        assert_eq!(config.port, 8080);
        assert_eq!(config.env, AppEnv::Development);
    }
}
