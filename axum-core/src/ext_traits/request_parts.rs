use crate::extract::FromRequestParts;
use http::request::Parts;
use std::future::Future;

mod sealed {
    pub trait Sealed {}
    impl Sealed for http::request::Parts {}
}

/// Extension trait that adds additional methods to [`Parts`].
pub trait RequestPartsExt: sealed::Sealed + Sized {
    /// Apply an extractor to this `Parts`.
    ///
    /// This is just a convenience for `E::from_request_parts(parts, &())`.
    fn extract<E>(&mut self) -> impl Future<Output = Result<E, E::Rejection>> + '_
    where
        E: FromRequestParts<()> + 'static;

    /// Apply an extractor that requires some state to this `Parts`.
    ///
    /// This is just a convenience for `E::from_request_parts(parts, state)`.
    fn extract_with_state<'a, E, S>(
        &'a mut self,
        state: &'a S,
    ) -> impl Future<Output = Result<E, E::Rejection>> + 'a
    where
        E: FromRequestParts<S>,
        S: Send + Sync;
}

impl RequestPartsExt for Parts {
    fn extract<E>(&mut self) -> impl Future<Output = Result<E, E::Rejection>> + '_
    where
        E: FromRequestParts<()> + 'static,
    {
        self.extract_with_state(&())
    }

    fn extract_with_state<'a, E, S>(
        &'a mut self,
        state: &'a S,
    ) -> impl Future<Output = Result<E, E::Rejection>> + 'a
    where
        E: FromRequestParts<S>,
        S: Send + Sync,
    {
        E::from_request_parts(self, state)
    }
}

#[cfg(test)]
mod tests {
    use std::convert::Infallible;

    use super::*;
    use crate::{
        ext_traits::tests::{RequiresState, State},
        extract::FromRef,
    };
    use http::{Method, Request};

    #[tokio::test]
    async fn extract_without_state() {
        let (mut parts, _) = Request::new(()).into_parts();

        let method: Method = parts.extract().await.unwrap();

        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn extract_with_state() {
        let (mut parts, _) = Request::new(()).into_parts();

        let state = "state".to_owned();

        let State(extracted_state): State<String> = parts
            .extract_with_state::<State<String>, String>(&state)
            .await
            .unwrap();

        assert_eq!(extracted_state, state);
    }

    // this stuff just needs to compile
    #[allow(dead_code)]
    struct WorksForCustomExtractor {
        method: Method,
        from_state: String,
    }

    impl<S> FromRequestParts<S> for WorksForCustomExtractor
    where
        S: Send + Sync,
        String: FromRef<S>,
    {
        type Rejection = Infallible;

        async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
            let RequiresState(from_state) = parts.extract_with_state(state).await?;
            let method = parts.extract().await?;

            Ok(Self { method, from_state })
        }
    }
}
