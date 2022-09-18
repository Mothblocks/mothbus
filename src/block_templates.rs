use std::{ffi::OsStr, future::Future, path::Path, pin::Pin};

use axum::response::{IntoResponse, Response};
use http::{Request, StatusCode};
use tower::Service;

#[derive(Clone)]
pub struct BlockTemplatesService<S> {
    inner: S,
}

impl<S> BlockTemplatesService<S> {
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S, ReqBody> Service<Request<ReqBody>> for BlockTemplatesService<S>
where
    S: Service<Request<ReqBody>, Response = Response>,
    S::Future: Unpin,
{
    type Response = Response;
    type Error = S::Error;
    type Future = BlockTemplatesServiceFuture<S, Request<ReqBody>>;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, request: Request<ReqBody>) -> Self::Future {
        if request.uri().path() == "/"
            || Path::new(request.uri().path())
                .extension()
                .and_then(OsStr::to_str)
                == Some("html")
        {
            BlockTemplatesServiceFuture::Forbidden
        } else {
            BlockTemplatesServiceFuture::Inner(self.inner.call(request))
        }
    }
}

pub enum BlockTemplatesServiceFuture<S: Service<R, Response = Response>, R> {
    Forbidden,
    Inner(<S as Service<R>>::Future),
}

impl<S, R> Future for BlockTemplatesServiceFuture<S, R>
where
    S: Service<R, Response = Response>,
    S::Future: Unpin,
{
    type Output = Result<Response, S::Error>;

    fn poll(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        match self.get_mut() {
            BlockTemplatesServiceFuture::Forbidden => std::task::Poll::Ready(Ok((
                StatusCode::FORBIDDEN,
                "you cannot access http resources through static",
            )
                .into_response())),

            BlockTemplatesServiceFuture::Inner(future) => Pin::new(future).poll(cx),
        }
    }
}
