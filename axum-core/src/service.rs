#![allow(missing_docs)] // temporary

use http::Request;
use std::{
    convert::Infallible,
    future::Future,
    task::{Context, Poll},
};
use tower_service::Service as TowerService;

use crate::response::{IntoResponse, Response};

pub trait Service<S, ReqBody> {
    type Future: Future<Output = Response>;
    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()>;
    fn call(&mut self, req: Request<ReqBody>, state: &S) -> Self::Future;
}

impl<T, S, ReqBody, Resp> Service<S, ReqBody> for T
where
    T: TowerService<Request<ReqBody>, Response = Resp, Error = Infallible>,
    Resp: IntoResponse,
{
    type Future = impl Future<Output = Response>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        TowerService::poll_ready(self, cx).map(|result| result.unwrap_or_else(|e| match e {}))
    }

    fn call(&mut self, req: Request<ReqBody>, _state: &S) -> Self::Future {
        let fut = TowerService::call(self, req);
        async move {
            match fut.await {
                Ok(res) => res.into_response(),
                Err(e) => match e {},
            }
        }
    }
}
