use crate::extract::{DefaultBodyLimitKind, FromRequest, FromRequestParts};
use http::Request;
use http_body::Limited;
use std::future::Future;

mod sealed {
    pub trait Sealed<B> {}
    impl<B> Sealed<B> for http::Request<B> {}
}

/// Extension trait that adds additional methods to [`Request`].
pub trait RequestExt<B>: sealed::Sealed<B> + Sized {
    /// Apply an extractor to this `Request`.
    ///
    /// This is just a convenience for `E::from_request(req, &())`.
    ///
    /// Note this consumes the request. Use [`RequestExt::extract_parts`] if you're not extracting
    /// the body and don't want to consume the request.
    fn extract<'a, E, M>(self) -> E::Future<'a>
    where
        B: 'a,
        E: FromRequest<(), B, M>;

    /// Apply an extractor that requires some state to this `Request`.
    ///
    /// This is just a convenience for `E::from_request(req, state)`.
    ///
    /// Note this consumes the request. Use [`RequestExt::extract_parts_with_state`] if you're not
    /// extracting the body and don't want to consume the request.
    fn extract_with_state<'a, E, S, M>(self, state: &'a S) -> E::Future<'a>
    where
        B: 'a,
        E: FromRequest<S, B, M>;

    #[doc(hidden)]
    #[rustfmt::skip]
    type ExtractPartsFuture<'a, E, S>:
        Future<Output = Result<E, <E as FromRequestParts<S>>::Rejection>> + 'a
    where
        Self: 'a,
        B: 'a,
        E: FromRequestParts<S>,
        S: 'a;

    /// Apply a parts extractor to this `Request`.
    ///
    /// This is just a convenience for `E::from_request_parts(parts, state)`.
    fn extract_parts<E>(&mut self) -> Self::ExtractPartsFuture<'_, E, ()>
    where
        E: FromRequestParts<()>;

    /// Apply a parts extractor that requires some state to this `Request`.
    ///
    /// This is just a convenience for `E::from_request_parts(parts, state)`.
    fn extract_parts_with_state<'a, E, S>(
        &'a mut self,
        state: &'a S,
    ) -> Self::ExtractPartsFuture<'a, E, S>
    where
        E: FromRequestParts<S>;

    /// Apply the [default body limit](crate::extract::DefaultBodyLimit).
    ///
    /// If it is disabled, return the request as-is in `Err`.
    fn with_limited_body(self) -> Result<Request<Limited<B>>, Request<B>>;

    /// Consumes the request, returning the body wrapped in [`Limited`] if a
    /// [default limit](crate::extract::DefaultBodyLimit) is in place, or not wrapped if the
    /// default limit is disabled.
    fn into_limited_body(self) -> Result<Limited<B>, B>;
}

impl<B> RequestExt<B> for Request<B> {
    fn extract<'a, E, M>(self) -> E::Future<'a>
    where
        B: 'a,
        E: FromRequest<(), B, M>,
    {
        self.extract_with_state::<E, _, M>(&())
    }

    fn extract_with_state<'a, E, S, M>(self, state: &'a S) -> E::Future<'a>
    where
        B: 'a,
        E: FromRequest<S, B, M>,
    {
        E::from_request(self, state)
    }

    type ExtractPartsFuture<'a, E, S> =
        impl Future<Output = Result<E, <E as FromRequestParts<S>>::Rejection>> + 'a
    where
        Self: 'a,
        B: 'a,
        E: FromRequestParts<S>,
        S: 'a;

    fn extract_parts<E>(&mut self) -> Self::ExtractPartsFuture<'_, E, ()>
    where
        E: FromRequestParts<()>,
    {
        self.extract_parts_with_state::<E, _>(&())
    }

    fn extract_parts_with_state<'a, E, S>(
        &'a mut self,
        state: &'a S,
    ) -> Self::ExtractPartsFuture<'a, E, S>
    where
        E: FromRequestParts<S>,
    {
        let mut req = Request::new(());
        *req.version_mut() = self.version();
        *req.method_mut() = self.method().clone();
        *req.uri_mut() = self.uri().clone();
        *req.headers_mut() = std::mem::take(self.headers_mut());
        *req.extensions_mut() = std::mem::take(self.extensions_mut());
        let (mut parts, _) = req.into_parts();

        async move {
            let result = E::from_request_parts(&mut parts, state).await;

            *self.version_mut() = parts.version;
            *self.method_mut() = parts.method.clone();
            *self.uri_mut() = parts.uri.clone();
            *self.headers_mut() = std::mem::take(&mut parts.headers);
            *self.extensions_mut() = std::mem::take(&mut parts.extensions);

            result
        }
    }

    fn with_limited_body(self) -> Result<Request<Limited<B>>, Request<B>> {
        // update docs in `axum-core/src/extract/default_body_limit.rs` and
        // `axum/src/docs/extract.md` if this changes
        const DEFAULT_LIMIT: usize = 2_097_152; // 2 mb

        match self.extensions().get::<DefaultBodyLimitKind>().copied() {
            Some(DefaultBodyLimitKind::Disable) => Err(self),
            Some(DefaultBodyLimitKind::Limit(limit)) => {
                Ok(self.map(|b| http_body::Limited::new(b, limit)))
            }
            None => Ok(self.map(|b| http_body::Limited::new(b, DEFAULT_LIMIT))),
        }
    }

    fn into_limited_body(self) -> Result<Limited<B>, B> {
        self.with_limited_body()
            .map(Request::into_body)
            .map_err(Request::into_body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        ext_traits::tests::{RequiresState, State},
        extract::FromRef,
    };
    use http::Method;
    use hyper::Body;

    #[tokio::test]
    async fn extract_without_state() {
        let req = Request::new(());

        let method = req.extract::<Method, _>().await.unwrap();

        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn extract_body_without_state() {
        let req = Request::new(Body::from("foobar"));

        let body = req.extract::<String, _>().await.unwrap();

        assert_eq!(body, "foobar");
    }

    #[tokio::test]
    async fn extract_with_state() {
        let req = Request::new(());

        let state = "state".to_owned();

        let State(extracted_state) = req
            .extract_with_state::<State<String>, _, _>(&state)
            .await
            .unwrap();

        assert_eq!(extracted_state, state);
    }

    #[tokio::test]
    async fn extract_parts_without_state() {
        let mut req = Request::builder().header("x-foo", "foo").body(()).unwrap();

        let method: Method = req.extract_parts().await.unwrap();

        assert_eq!(method, Method::GET);
        assert_eq!(req.headers()["x-foo"], "foo");
    }

    #[tokio::test]
    async fn extract_parts_with_state() {
        let mut req = Request::builder().header("x-foo", "foo").body(()).unwrap();

        let state = "state".to_owned();

        let State(extracted_state): State<String> =
            req.extract_parts_with_state(&state).await.unwrap();

        assert_eq!(extracted_state, state);
        assert_eq!(req.headers()["x-foo"], "foo");
    }

    // this stuff just needs to compile
    #[allow(dead_code)]
    struct WorksForCustomExtractor {
        method: Method,
        from_state: String,
        body: String,
    }

    impl<S, B> FromRequest<S, B> for WorksForCustomExtractor
    where
        S: Send + Sync,
        String: FromRef<S> + FromRequest<(), B>,
    {
        type Future<'a> = impl Future<Output = Result<Self, Self::Rejection>> + 'a
        where
            B: 'a,
            S: 'a;
        type Rejection = <String as FromRequest<(), B>>::Rejection;

        fn from_request(mut req: Request<B>, state: &S) -> Self::Future<'_> {
            async move {
                let RequiresState(from_state) = req.extract_parts_with_state(state).await.unwrap();
                let method = req.extract_parts().await.unwrap();
                let body = req.extract::<String, _>().await?;

                Ok(Self {
                    method,
                    from_state,
                    body,
                })
            }
        }
    }
}
