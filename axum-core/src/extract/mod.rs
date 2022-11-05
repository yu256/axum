//! Types and traits for extracting data from requests.
//!
//! See [`axum::extract`] for more details.
//!
//! [`axum::extract`]: https://docs.rs/axum/latest/axum/extract/index.html

use crate::response::IntoResponse;
use http::{request::Parts, Request};
use std::{convert::Infallible, future::Future};

pub mod rejection;

mod default_body_limit;
mod from_ref;
mod request_parts;
mod tuple;

pub(crate) use self::default_body_limit::DefaultBodyLimitKind;
pub use self::{default_body_limit::DefaultBodyLimit, from_ref::FromRef};

mod private {
    #[derive(Debug, Clone, Copy)]
    pub enum ViaParts {}

    #[derive(Debug, Clone, Copy)]
    pub enum ViaRequest {}
}

/// Types that can be created from request parts.
///
/// Extractors that implement `FromRequestParts` cannot consume the request body and can thus be
/// run in any order for handlers.
///
/// If your extractor needs to consume the request body then you should implement [`FromRequest`]
/// and not [`FromRequestParts`].
///
/// See [`axum::extract`] for more general docs about extraxtors.
///
/// [`axum::extract`]: https://docs.rs/axum/0.6.0-rc.2/axum/extract/index.html
pub trait FromRequestParts<S>: Sized {
    /// If the extractor fails it'll use this "rejection" type. A rejection is
    /// a kind of error that can be converted into a response.
    type Rejection: IntoResponse;

    /// Perform the extraction.
    fn from_request_parts<'a>(
        parts: &'a mut Parts,
        state: &'a S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + 'a;
}

/// Types that can be created from requests.
///
/// Extractors that implement `FromRequest` can consume the request body and can thus only be run
/// once for handlers.
///
/// If your extractor doesn't need to consume the request body then you should implement
/// [`FromRequestParts`] and not [`FromRequest`].
///
/// See [`axum::extract`] for more general docs about extraxtors.
///
/// # What is the `B` type parameter?
///
/// `FromRequest` is generic over the request body (the `B` in
/// [`http::Request<B>`]). This is to allow `FromRequest` to be usable with any
/// type of request body. This is necessary because some middleware change the
/// request body, for example to add timeouts.
///
/// If you're writing your own `FromRequest` that wont be used outside your
/// application, and not using any middleware that changes the request body, you
/// can most likely use `axum::body::Body`.
///
/// If you're writing a library that's intended for others to use, it's recommended
/// to keep the generic type parameter:
///
/// ```rust
/// use axum::{
///
///     extract::FromRequest,
///     http::Request,
/// };
///
/// struct MyExtractor;
///
/// impl<S, B> FromRequest<S, B> for MyExtractor
/// where
///     B: Send + 'static,
///     S: Send + Sync,
/// {
///     type Rejection = http::StatusCode;
///
///     fn from_request(req: Request<B>, state: &S) -> Result<Self, Self::Rejection> {
///         // ...
///         # unimplemented!()
///     }
/// }
/// ```
///
/// This ensures your extractor is as flexible as possible.
///
/// [`http::Request<B>`]: http::Request
/// [`axum::extract`]: https://docs.rs/axum/0.6.0-rc.2/axum/extract/index.html
pub trait FromRequest<S, B, M = private::ViaRequest>: Sized {
    /// If the extractor fails it'll use this "rejection" type. A rejection is
    /// a kind of error that can be converted into a response.
    type Rejection: IntoResponse;

    /// Perform the extraction.
    fn from_request(
        req: Request<B>,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + '_;
}

impl<S, B, T> FromRequest<S, B, private::ViaParts> for T
where
    B: Send + 'static,
    S: Send + Sync,
    T: FromRequestParts<S>,
{
    type Rejection = <Self as FromRequestParts<S>>::Rejection;

    fn from_request(
        req: Request<B>,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + '_ {
        async move {
            let (mut parts, _) = req.into_parts();
            Self::from_request_parts(&mut parts, state).await
        }
    }
}

impl<S, T> FromRequestParts<S> for Option<T>
where
    T: FromRequestParts<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts<'a>(
        parts: &'a mut Parts,
        state: &'a S,
    ) -> impl Future<Output = Result<Option<T>, Self::Rejection>> + Send + 'a {
        async move { Ok(T::from_request_parts(parts, state).await.ok()) }
    }
}

impl<S, T, B> FromRequest<S, B> for Option<T>
where
    T: FromRequest<S, B>,
    B: Send + 'static,
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request(
        req: Request<B>,
        state: &S,
    ) -> impl Future<Output = Result<Option<T>, Self::Rejection>> + Send + '_ {
        async move { Ok(T::from_request(req, state).await.ok()) }
    }
}

impl<S, T> FromRequestParts<S> for Result<T, T::Rejection>
where
    T: FromRequestParts<S>,
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request_parts<'a>(
        parts: &'a mut Parts,
        state: &'a S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + 'a {
        async move { Ok(T::from_request_parts(parts, state).await) }
    }
}

impl<S, T, B> FromRequest<S, B> for Result<T, T::Rejection>
where
    T: FromRequest<S, B>,
    B: Send + 'static,
    S: Send + Sync,
{
    type Rejection = Infallible;

    fn from_request(
        req: Request<B>,
        state: &S,
    ) -> impl Future<Output = Result<Self, Self::Rejection>> + Send + '_ {
        async move { Ok(T::from_request(req, state).await) }
    }
}
