mod async_body;

use anyhow::Result;
use futures::future::BoxFuture;
use http::HeaderValue;

pub use http::{self, Method, Request, Response, StatusCode, Uri, request::Builder};
pub use url::Url;

pub use async_body::{AsyncBody, Inner};

#[derive(Default, Debug, Clone, PartialEq, Eq, Hash)]
pub enum RedirectPolicy {
    #[default]
    NoFollow,
    FollowLimit(u32),
    FollowAll,
}

pub trait HttpRequestExt {
    fn when(self, condition: bool, then: impl FnOnce(Self) -> Self) -> Self
    where
        Self: Sized,
    {
        if condition { then(self) } else { self }
    }

    fn when_some<T>(self, option: Option<T>, then: impl FnOnce(Self, T) -> Self) -> Self
    where
        Self: Sized,
    {
        match option {
            Some(value) => then(self, value),
            None => self,
        }
    }

    fn follow_redirects(self, follow: RedirectPolicy) -> Self;
}

impl HttpRequestExt for http::request::Builder {
    fn follow_redirects(self, follow: RedirectPolicy) -> Self {
        self.extension(follow)
    }
}

pub trait HttpClient: 'static + Send + Sync {
    fn user_agent(&self) -> Option<&HeaderValue>;

    fn proxy(&self) -> Option<&Url>;

    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>>>;

    fn get(
        &self,
        uri: &str,
        body: AsyncBody,
        follow_redirects: bool,
    ) -> BoxFuture<'static, Result<Response<AsyncBody>>> {
        let request = Builder::new()
            .uri(uri)
            .follow_redirects(if follow_redirects {
                RedirectPolicy::FollowAll
            } else {
                RedirectPolicy::NoFollow
            })
            .body(body);

        match request {
            Ok(request) => self.send(request),
            Err(error) => Box::pin(async move { Err(error.into()) }),
        }
    }
}
