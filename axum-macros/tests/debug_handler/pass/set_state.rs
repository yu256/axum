use axum::extract::{FromRef, FromRequest};
use axum::http::Request;
use axum_macros::debug_handler;

#[debug_handler(state = AppState)]
async fn handler(_: A) {}

#[derive(Clone)]
struct AppState;

struct A;

impl<S, B> FromRequest<S, B> for A
where
    B: Send + 'static,
    S: Send + Sync,
    AppState: FromRef<S>,
{
    type Rejection = ();

    fn from_request(_req: Request<B>, _state: &S) -> Result<Self, Self::Rejection> {
        unimplemented!()
    }
}

fn main() {}
