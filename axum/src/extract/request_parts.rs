use super::{Extension, FromRequest, FromRequestParts};
use crate::{
    body::{Body, Bytes, HttpBody},
    BoxError, Error,
};
use futures_util::stream::Stream;
use http::{request::Parts, Request, Uri};
use std::{
    convert::Infallible,
    fmt,
    future::Future,
    pin::Pin,
    task::{Context, Poll},
};
use sync_wrapper::SyncWrapper;

/// Extractor that gets the original request URI regardless of nesting.
///
/// This is necessary since [`Uri`](http::Uri), when used as an extractor, will
/// have the prefix stripped if used in a nested service.
///
/// # Example
///
/// ```
/// use axum::{
///     routing::get,
///     Router,
///     extract::OriginalUri,
///     http::Uri
/// };
///
/// let api_routes = Router::new()
///     .route(
///         "/users",
///         get(|uri: Uri, OriginalUri(original_uri): OriginalUri| async {
///             // `uri` is `/users`
///             // `original_uri` is `/api/users`
///         }),
///     );
///
/// let app = Router::new().nest("/api", api_routes);
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
///
/// # Extracting via request extensions
///
/// `OriginalUri` can also be accessed from middleware via request extensions.
/// This is useful for example with [`Trace`](tower_http::trace::Trace) to
/// create a span that contains the full path, if your service might be nested:
///
/// ```
/// use axum::{
///     Router,
///     extract::OriginalUri,
///     http::Request,
///     routing::get,
/// };
/// use tower_http::trace::TraceLayer;
///
/// let api_routes = Router::new()
///     .route("/users/:id", get(|| async { /* ... */ }))
///     .layer(
///         TraceLayer::new_for_http().make_span_with(|req: &Request<_>| {
///             let path = if let Some(path) = req.extensions().get::<OriginalUri>() {
///                 // This will include `/api`
///                 path.0.path().to_owned()
///             } else {
///                 // The `OriginalUri` extension will always be present if using
///                 // `Router` unless another extractor or middleware has removed it
///                 req.uri().path().to_owned()
///             };
///             tracing::info_span!("http-request", %path)
///         }),
///     );
///
/// let app = Router::new().nest("/api", api_routes);
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
#[cfg(feature = "original-uri")]
#[derive(Debug, Clone)]
pub struct OriginalUri(pub Uri);

#[cfg(feature = "original-uri")]
impl<S> FromRequestParts<S> for OriginalUri {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        S: 'a;
    type Rejection = Infallible;

    fn from_request_parts<'a>(parts: &'a mut Parts, state: &'a S) -> Self::Future<'a> {
        async move {
            let uri = Extension::<Self>::from_request_parts(parts, state)
                .await
                .unwrap_or_else(|_| Extension(OriginalUri(parts.uri.clone())))
                .0;
            Ok(uri)
        }
    }
}

/// Extractor that extracts the request body as a [`Stream`].
///
/// Since extracting the request body requires consuming it, the `BodyStream` extractor must be
/// *last* if there are multiple extractors in a handler.
/// See ["the order of extractors"][order-of-extractors]
///
/// [order-of-extractors]: crate::extract#the-order-of-extractors
///
/// # Example
///
/// ```rust,no_run
/// use axum::{
///     extract::BodyStream,
///     routing::get,
///     Router,
/// };
/// use futures::StreamExt;
///
/// async fn handler(mut stream: BodyStream) {
///     while let Some(chunk) = stream.next().await {
///         // ...
///     }
/// }
///
/// let app = Router::new().route("/users", get(handler));
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
///
/// [`Stream`]: https://docs.rs/futures/latest/futures/stream/trait.Stream.html
/// [`body::Body`]: crate::body::Body
pub struct BodyStream<B>(SyncWrapper<BodyStreamInner<B>>)
where
    B: HttpBody;

type BodyStreamInner<B: HttpBody> = http_body::combinators::MapErr<
    http_body::combinators::MapData<B, fn(B::Data) -> Bytes>,
    fn(B::Error) -> Error,
>;

impl<B> Stream for BodyStream<B>
where
    // FIXME: Unpin not needed if pin projection is used.
    //        Should replace the combinator stack with a manual HttpBody impl.
    B: HttpBody + Unpin,
{
    type Item = Result<Bytes, Error>;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        Pin::new(self.0.get_mut()).poll_data(cx)
    }
}

impl<S, B> FromRequest<S, B> for BodyStream<B>
where
    B: HttpBody,
    B::Data: Into<Bytes>,
    B::Error: Into<BoxError>,
{
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        B: 'a,
        S: 'a;
    type Rejection = Infallible;

    fn from_request(req: Request<B>, _: &S) -> Self::Future<'_> {
        async move {
            let body = req
                .into_body()
                .map_data(Into::into as fn(B::Data) -> Bytes)
                .map_err((|err| Error::new(err.into())) as fn(B::Error) -> Error);
            let stream = BodyStream(SyncWrapper::new(body));
            Ok(stream)
        }
    }
}

impl<B> fmt::Debug for BodyStream<B>
where
    B: HttpBody,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("BodyStream").finish()
    }
}

#[test]
fn body_stream_traits() {
    use http_body::Full;

    crate::test_helpers::assert_send::<BodyStream<Full<Bytes>>>();
    crate::test_helpers::assert_sync::<BodyStream<Full<Bytes>>>();
}

/// Extractor that extracts the raw request body.
///
/// Since extracting the raw request body requires consuming it, the `RawBody` extractor must be
/// *last* if there are multiple extractors in a handler. See ["the order of extractors"][order-of-extractors]
///
/// [order-of-extractors]: crate::extract#the-order-of-extractors
///
/// # Example
///
/// ```rust,no_run
/// use axum::{
///     extract::RawBody,
///     routing::get,
///     Router,
/// };
/// use futures::StreamExt;
///
/// async fn handler(RawBody(body): RawBody) {
///     // ...
/// }
///
/// let app = Router::new().route("/users", get(handler));
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
///
/// [`body::Body`]: crate::body::Body
#[derive(Debug, Default, Clone)]
pub struct RawBody<B = Body>(pub B);

impl<S, B> FromRequest<S, B> for RawBody<B> {
    type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
    where
        B: 'a,
        S: 'a;
    type Rejection = Infallible;

    fn from_request(req: Request<B>, _state: &S) -> Self::Future<'_> {
        async move { Ok(Self(req.into_body())) }
    }
}

#[cfg(test)]
mod tests {
    use crate::{extract::Extension, routing::get, test_helpers::*, Router};
    use http::{Method, StatusCode};

    #[tokio::test]
    async fn extract_request_parts() {
        #[derive(Clone)]
        struct Ext;

        async fn handler(parts: http::request::Parts) {
            assert_eq!(parts.method, Method::GET);
            assert_eq!(parts.uri, "/");
            assert_eq!(parts.version, http::Version::HTTP_11);
            assert_eq!(parts.headers["x-foo"], "123");
            parts.extensions.get::<Ext>().unwrap();
        }

        let client = TestClient::new(Router::new().route("/", get(handler)).layer(Extension(Ext)));

        let res = client.get("/").header("x-foo", "123").send().await;
        assert_eq!(res.status(), StatusCode::OK);
    }
}
