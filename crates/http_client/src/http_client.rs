mod async_body;

use futures::future::BoxFuture;
use http::HeaderValue;

#[cfg(any(test, feature = "test"))]
use parking_lot::Mutex;

#[cfg(any(test, feature = "test"))]
use std::{any::type_name, fmt, sync::Arc};

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
    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>>;

    fn user_agent(&self) -> Option<&HeaderValue>;

    fn proxy(&self) -> Option<&Url>;

    fn get(
        &self,
        uri: &str,
        body: AsyncBody,
        follow_redirects: bool,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
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

#[cfg(any(test, feature = "test"))]
type FakeHttpHandler = Arc<
    dyn Fn(Request<AsyncBody>) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>>
        + Send
        + Sync
        + 'static,
>;

#[cfg(any(test, feature = "test"))]
pub struct FakeHttpClient {
    handler: Mutex<FakeHttpHandler>,
    user_agent: HeaderValue,
}

#[cfg(any(test, feature = "test"))]
impl FakeHttpClient {
    pub fn create<Fut, F>(handler: F) -> Arc<Self>
    where
        Fut: futures::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static,
        F: Fn(Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        Arc::new(Self {
            handler: Mutex::new(Arc::new(move |request| Box::pin(handler(request)))),
            user_agent: HeaderValue::from_static(type_name::<Self>()),
        })
    }

    pub fn with_response(status: StatusCode) -> Arc<Self> {
        log::warn!("Using fake HTTP client with {status} response");
        Self::create(move |_| async move {
            let mut response = Response::new(AsyncBody::default());
            *response.status_mut() = status;
            Ok(response)
        })
    }

    pub fn replace_handler<Fut, F>(&self, new_handler: F)
    where
        Fut: futures::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static,
        F: Fn(FakeHttpHandler, Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        let mut handler = self.handler.lock();
        let old_handler = handler.clone();
        *handler = Arc::new(move |request| Box::pin(new_handler(old_handler.clone(), request)));
    }
}

#[cfg(any(test, feature = "test"))]
impl fmt::Debug for FakeHttpClient {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_struct("FakeHttpClient").finish()
    }
}

#[cfg(any(test, feature = "test"))]
impl HttpClient for FakeHttpClient {
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        let handler = self.handler.lock().clone();
        handler(request)
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}
