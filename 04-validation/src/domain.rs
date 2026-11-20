use serde::Serialize;
use validator::ValidateEmail;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct UserName(String);

impl TryFrom<String> for UserName {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let name = value.trim();
        if name.is_empty() {
            return Err("cannot be empty".into());
        }
        if name.chars().count() > 50 {
            return Err("must be at most 50 characters".into());
        }
        Ok(Self(name.to_string()))
    }
}

impl UserName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Email(String);

impl TryFrom<String> for Email {
    type Error = String;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        let email = value.trim().to_lowercase();
        if !email.validate_email() {
            return Err("must be a valid email".into());
        }
        Ok(Self(email))
    }
}

impl Email {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
