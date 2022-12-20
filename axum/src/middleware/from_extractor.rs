use crate::{
    extract::FromRequestParts,
    response::{IntoResponse, Response},
    Service,
};
use futures_util::{future::BoxFuture, ready};
use http::Request;
use pin_project_lite::pin_project;
use std::{
    fmt,
    future::Future,
    marker::PhantomData,
    pin::Pin,
    task::{Context, Poll},
};
use tower_layer::Layer;

/// Create a middleware from an extractor.
///
/// If the extractor succeeds the value will be discarded and the inner service
/// will be called. If the extractor fails the rejection will be returned and
/// the inner service will _not_ be called.
///
/// This can be used to perform validation of requests if the validation doesn't
/// produce any useful output, and run the extractor for several handlers
/// without repeating it in the function signature.
///
/// Note that if the extractor consumes the request body, as `String` or
/// [`Bytes`] does, an empty body will be left in its place. Thus wont be
/// accessible to subsequent extractors or handlers.
///
/// # Example
///
/// ```rust
/// use axum::{
///     extract::FromRequestParts,
///     middleware::from_extractor,
///     routing::{get, post},
///     Router,
///     http::{header, StatusCode, request::Parts},
/// };
/// use async_trait::async_trait;
///
/// // An extractor that performs authorization.
/// struct RequireAuth;
///
/// #[async_trait]
/// impl<S> FromRequestParts<S> for RequireAuth
/// where
///     S: Send + Sync,
/// {
///     type Rejection = StatusCode;
///
///     async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
///         let auth_header = parts
///             .headers
///             .get(header::AUTHORIZATION)
///             .and_then(|value| value.to_str().ok());
///
///         match auth_header {
///             Some(auth_header) if token_is_valid(auth_header) => {
///                 Ok(Self)
///             }
///             _ => Err(StatusCode::UNAUTHORIZED),
///         }
///     }
/// }
///
/// fn token_is_valid(token: &str) -> bool {
///     // ...
///     # false
/// }
///
/// async fn handler() {
///     // If we get here the request has been authorized
/// }
///
/// async fn other_handler() {
///     // If we get here the request has been authorized
/// }
///
/// let app = Router::new()
///     .route("/", get(handler))
///     .route("/foo", post(other_handler))
///     // The extractor will run before all routes
///     .route_layer(from_extractor::<RequireAuth>());
/// # async {
/// # axum::Server::bind(&"".parse().unwrap()).serve(app.into_make_service()).await.unwrap();
/// # };
/// ```
///
/// [`Bytes`]: bytes::Bytes
pub fn from_extractor<E>() -> FromExtractorLayer<E> {
    FromExtractorLayer {
        _marker: PhantomData,
    }
}

/// [`Layer`] that applies [`FromExtractor`] that runs an extractor and
/// discards the value.
///
/// See [`from_extractor`] for more details.
///
/// [`Layer`]: tower::Layer
pub struct FromExtractorLayer<E> {
    _marker: PhantomData<fn() -> E>,
}

impl<E> Clone for FromExtractorLayer<E> {
    fn clone(&self) -> Self {
        Self {
            _marker: PhantomData,
        }
    }
}

impl<E> fmt::Debug for FromExtractorLayer<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FromExtractorLayer")
            .field("extractor", &format_args!("{}", std::any::type_name::<E>()))
            .finish()
    }
}

impl<E, T> Layer<T> for FromExtractorLayer<E> {
    type Service = FromExtractor<T, E>;

    fn layer(&self, inner: T) -> Self::Service {
        FromExtractor {
            inner,
            _extractor: PhantomData,
        }
    }
}

/// Middleware that runs an extractor and discards the value.
///
/// See [`from_extractor`] for more details.
pub struct FromExtractor<T, E> {
    inner: T,
    _extractor: PhantomData<fn() -> E>,
}

#[test]
fn traits() {
    use crate::test_helpers::*;
    assert_send::<FromExtractor<(), NotSendSync>>();
    assert_sync::<FromExtractor<(), NotSendSync>>();
}

impl<T, E> Clone for FromExtractor<T, E>
where
    T: Clone,
{
    fn clone(&self) -> Self {
        Self {
            inner: self.inner.clone(),
            _extractor: PhantomData,
        }
    }
}

impl<T, E> fmt::Debug for FromExtractor<T, E>
where
    T: fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FromExtractor")
            .field("inner", &self.inner)
            .field("extractor", &format_args!("{}", std::any::type_name::<E>()))
            .finish()
    }
}

impl<T, E, B, S> Service<S, B> for FromExtractor<T, E>
where
    E: FromRequestParts<S> + 'static,
    B: Send + 'static,
    T: Service<S, B> + Clone,
    S: Clone + Send + Sync + 'static,
{
    type Future = ResponseFuture<B, T, E, S>;

    #[inline]
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<B>, st: &S) -> Self::Future {
        let state = st.to_owned();
        let extract_future = Box::pin(async move {
            let (mut parts, body) = req.into_parts();
            let extracted = E::from_request_parts(&mut parts, &state).await;
            let req = Request::from_parts(parts, body);
            (req, extracted)
        });

        ResponseFuture {
            state: State::Extracting {
                st: st.clone(),
                future: extract_future,
            },
            svc: Some(self.inner.clone()),
        }
    }
}

pin_project! {
    /// Response future for [`FromExtractor`].
    #[allow(missing_debug_implementations)]
    pub struct ResponseFuture<B, T, E, S>
    where
        E: FromRequestParts<S>,
        T: Service<S, B>,
    {
        #[pin]
        state: State<B, T, E, S>,
        svc: Option<T>,
    }
}

pin_project! {
    #[project = StateProj]
    enum State<B, T, E, S>
    where
        E: FromRequestParts<S>,
        T: Service<S, B>,
    {
        Extracting {
            st: S,
            future: BoxFuture<'static, (Request<B>, Result<E, E::Rejection>)>,
        },
        Call { #[pin] future: T::Future },
    }
}

impl<B, T, E, S> Future for ResponseFuture<B, T, E, S>
where
    E: FromRequestParts<S>,
    T: Service<S, B>,
{
    type Output = Response;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            let mut this = self.as_mut().project();

            let new_state = match this.state.as_mut().project() {
                StateProj::Extracting { future, st } => {
                    let (req, extracted) = ready!(future.as_mut().poll(cx));

                    match extracted {
                        Ok(_) => {
                            let mut svc = this.svc.take().expect("future polled after completion");
                            let future = svc.call(req, st);
                            State::Call { future }
                        }
                        Err(err) => {
                            let res = err.into_response();
                            return Poll::Ready(res);
                        }
                    }
                }
                StateProj::Call { future } => {
                    return future.poll(cx);
                }
            };

            this.state.set(new_state);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{async_trait, handler::Handler, routing::get, test_helpers::*, Router};
    use axum_core::extract::FromRef;
    use http::{header, request::Parts, StatusCode};
    use tower_http::limit::RequestBodyLimitLayer;

    #[tokio::test]
    async fn test_from_extractor() {
        #[derive(Clone)]
        struct Secret(&'static str);

        struct RequireAuth;

        #[async_trait::async_trait]
        impl<S> FromRequestParts<S> for RequireAuth
        where
            S: Send + Sync,
            Secret: FromRef<S>,
        {
            type Rejection = StatusCode;

            async fn from_request_parts(
                parts: &mut Parts,
                state: &S,
            ) -> Result<Self, Self::Rejection> {
                let Secret(secret) = Secret::from_ref(state);
                if let Some(auth) = parts
                    .headers
                    .get(header::AUTHORIZATION)
                    .and_then(|v| v.to_str().ok())
                {
                    if auth == secret {
                        return Ok(Self);
                    }
                }

                Err(StatusCode::UNAUTHORIZED)
            }
        }

        async fn handler() {}

        let state = Secret("secret");
        let app = Router::new().route("/", get(handler.layer(from_extractor())));

        let client = TestClient::new(app);

        let res = client.get("/").send().await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED);

        let res = client
            .get("/")
            .header(http::header::AUTHORIZATION, "secret")
            .send()
            .await;
        assert_eq!(res.status(), StatusCode::OK);
    }

    // just needs to compile
    #[allow(dead_code)]
    fn works_with_request_body_limit() {
        struct MyExtractor;

        #[async_trait]
        impl<S> FromRequestParts<S> for MyExtractor
        where
            S: Send + Sync,
        {
            type Rejection = std::convert::Infallible;

            async fn from_request_parts(
                _parts: &mut Parts,
                _state: &S,
            ) -> Result<Self, Self::Rejection> {
                unimplemented!()
            }
        }

        let _: Router = Router::new()
            .layer(from_extractor::<MyExtractor>())
            .layer(RequestBodyLimitLayer::new(1));
    }
}
