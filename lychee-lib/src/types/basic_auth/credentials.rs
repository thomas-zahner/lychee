use async_trait::async_trait;
use std::str::FromStr;

use headers::authorization::Credentials;
use headers::{Authorization, authorization::Basic};
use http::header::AUTHORIZATION;
use reqwest::Request;
use serde::Deserialize;
use thiserror::Error;

use crate::chain::{ChainResult, Handler};

#[derive(Copy, Clone, Debug, Error, PartialEq)]
pub enum BasicAuthCredentialsParseError {
    #[error("Invalid basic auth credentials syntax")]
    InvalidSyntax,

    #[error("Missing basic auth password")]
    MissingPassword,

    #[error("Missing basic auth username")]
    MissingUsername,

    #[error(
        "Too many values separated by colon. Expected 2, got {0}. Valid form is '<username>:<password>'"
    )]
    TooManyParts(usize),
}

/// [`BasicAuthCredentials`] contains a pair of basic auth values consisting of
/// a username and password.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq, Hash)]
pub struct BasicAuthCredentials {
    /// Basic auth username
    pub username: String,

    /// Basic auth password
    pub password: String,
}

impl FromStr for BasicAuthCredentials {
    type Err = BasicAuthCredentialsParseError;

    fn from_str(credentials: &str) -> Result<Self, Self::Err> {
        let parts: Vec<_> = credentials.trim().split(':').collect();

        if parts.len() <= 1 {
            return Err(BasicAuthCredentialsParseError::InvalidSyntax);
        }

        if parts.len() > 2 {
            return Err(BasicAuthCredentialsParseError::TooManyParts(parts.len()));
        }

        if parts[0].is_empty() {
            return Err(BasicAuthCredentialsParseError::MissingUsername);
        }

        if parts[1].is_empty() {
            return Err(BasicAuthCredentialsParseError::MissingPassword);
        }

        Ok(Self {
            username: parts[0].to_string(),
            password: parts[1].to_string(),
        })
    }
}

impl BasicAuthCredentials {
    /// Returns the credentials as [`Authorization<Basic>`].
    #[must_use]
    pub fn to_authorization(&self) -> Authorization<Basic> {
        Authorization::basic(&self.username, &self.password)
    }

    /// Append the credentials as headers to a `Request`
    pub fn append_to_request(&self, request: &mut Request) {
        request
            .headers_mut()
            .append(AUTHORIZATION, self.to_authorization().0.encode());
    }
}

#[async_trait]
impl<Response> Handler<Request, Response> for Option<BasicAuthCredentials> {
    async fn handle(&mut self, mut request: Request) -> ChainResult<Request, Response> {
        if let Some(credentials) = self {
            credentials.append_to_request(&mut request);
        }

        ChainResult::Next(request)
    }
}
