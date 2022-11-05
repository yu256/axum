use super::FromRequestParts;
use http::request::Parts;
use std::{convert::Infallible, future::Future};

/// Extractor that extracts the raw query string, without parsing it.
///
/// # Example
///
/// ```rust,no_run
/// use axum::{
///     extract::RawQuery,
///     routing::get,
///     Router,
/// };
/// use futures::StreamExt;
///
/// async fn handler(RawQuery(query): RawQuery) {
///     // ...
/// }
///
/// let app = Router::new().route("/users", get(handler));
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
#[derive(Debug)]
pub struct RawQuery(pub Option<String>);

impl<S> FromRequestParts<S> for RawQuery
where
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts<'a>(
        parts: &'a mut Parts,
        _state: &'a S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + 'a {
        async move {
            let query = parts.uri.query().map(|query| query.to_owned());
            Ok(Self(query))
        }
    }
}
