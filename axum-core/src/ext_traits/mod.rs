pub(crate) mod request;
pub(crate) mod request_parts;

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, future::Future};

    use crate::extract::{FromRef, FromRequestParts};
    use http::request::Parts;

    #[derive(Debug, Default, Clone, Copy)]
    pub(crate) struct State<S>(pub(crate) S);

    impl<OuterState, InnerState> FromRequestParts<OuterState> for State<InnerState>
    where
        OuterState: Send + Sync,
        InnerState: FromRef<OuterState>,
    {
        type Rejection = Infallible;

        fn from_request_parts<'a>(
            _parts: &'a mut Parts,
            state: &'a OuterState,
        ) -> Self::Future<'a> {
            async move {
                let inner_state = InnerState::from_ref(state);
                Ok(Self(inner_state))
            }
        }
    }

    // some extractor that requires the state, such as `SignedCookieJar`
    pub(crate) struct RequiresState(pub(crate) String);

    impl<S> FromRequestParts<S> for RequiresState
    where
        S: Send + Sync,
        String: FromRef<S>,
    {
        type Rejection = Infallible;

        fn from_request_parts<'a>(_parts: &'a mut Parts, state: &'a S) -> Self::Future<'a> {
            async move { Ok(Self(String::from_ref(state))) }
        }
    }
}
