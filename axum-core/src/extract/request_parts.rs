use super::{rejection::*, FromRequest, FromRequestParts};
use crate::{BoxError, RequestExt};
use bytes::Bytes;
use http::{request::Parts, HeaderMap, Method, Request, Uri, Version};
use std::{convert::Infallible, future::Future};

impl<S, B> FromRequest<S, B> for Request<B> {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        B: 'a,
        S: 'a;
    type Rejection = Infallible;

    fn from_request(req: Request<B>, _state: &S) -> Self::Future<'_> {
        async move { Ok(req) }
    }
}

impl<S> FromRequestParts<S> for Method {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request_parts<'a>(parts: &'a mut Parts, _: &'a S) -> Self::Future<'a> {
        async move { Ok(parts.method.clone()) }
    }
}

impl<S> FromRequestParts<S> for Uri {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request_parts<'a>(parts: &'a mut Parts, _: &'a S) -> Self::Future<'a> {
        async move { Ok(parts.uri.clone()) }
    }
}

impl<S> FromRequestParts<S> for Version {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request_parts<'a>(parts: &'a mut Parts, _: &'a S) -> Self::Future<'a> {
        async move { Ok(parts.version) }
    }
}

/// Clone the headers from the request.
///
/// Prefer using [`TypedHeader`] to extract only the headers you need.
///
/// [`TypedHeader`]: https://docs.rs/axum/latest/axum/extract/struct.TypedHeader.html
impl<S> FromRequestParts<S> for HeaderMap {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request_parts<'a>(parts: &'a mut Parts, _: &'a S) -> Self::Future<'a> {
        async move { Ok(parts.headers.clone()) }
    }
}

impl<S, B> FromRequest<S, B> for Bytes
where
    B: http_body::Body,
    B::Error: Into<BoxError>,
{
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        B: 'a,
        S: 'a;
    type Rejection = BytesRejection;

    fn from_request(req: Request<B>, _state: &S) -> Self::Future<'_> {
        async move {
            let bytes = match req.into_limited_body() {
                Ok(limited_body) => crate::body::to_bytes(limited_body)
                    .await
                    .map_err(FailedToBufferBody::from_err)?,
                Err(unlimited_body) => crate::body::to_bytes(unlimited_body)
                    .await
                    .map_err(FailedToBufferBody::from_err)?,
            };

            Ok(bytes)
        }
    }
}

impl<S, B> FromRequest<S, B> for String
where
    B: http_body::Body + Send + 'static,
    B::Data: Send,
    B::Error: Into<BoxError>,
{
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = StringRejection;

    fn from_request(req: Request<B>, _state: &S) -> Self::Future<'_> {
        async move {
            let bytes = Bytes::from_request(req, &())
                .await
                .map_err(|err| match err {
                    BytesRejection::FailedToBufferBody(inner) => {
                        StringRejection::FailedToBufferBody(inner)
                    }
                })?;

            let string = std::str::from_utf8(&bytes)
                .map_err(InvalidUtf8::from_err)?
                .to_owned();

            Ok(string)
        }
    }
}

impl<S, B> FromRequest<S, B> for Parts
where
    B: Send + 'static,
{
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request(req: Request<B>, _state: &S) -> Self::Future<'_> {
        async move { Ok(req.into_parts().0) }
    }
}
