mod async_body;

use derive_more::Deref;
use futures::future::BoxFuture;
use http::HeaderValue;
use parking_lot::Mutex;
use std::sync::Arc;
#[cfg(any(test, feature = "test"))]
use std::{any::type_name, fmt};

pub use http::{self, Method, Request, Response, StatusCode, Uri, request::Builder};
pub use url::Url;

pub use async_body::{AsyncBody, Inner};

#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
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

#[derive(Deref)]
pub struct HttpClientWithUrl {
    base_url: Mutex<String>,
    #[deref]
    client: Arc<dyn HttpClient>,
}

impl HttpClientWithUrl {
    pub fn new(client: Arc<dyn HttpClient>, base_url: impl Into<String>) -> Self {
        Self {
            base_url: Mutex::new(base_url.into()),
            client,
        }
    }

    pub fn base_url(&self) -> String {
        self.base_url.lock().clone()
    }

    pub fn set_base_url(&self, base_url: impl Into<String>) {
        *self.base_url.lock() = base_url.into();
    }

    pub fn build_url(&self, path: &str) -> String {
        format!("{}{}", self.base_url(), path)
    }
}

impl HttpClient for HttpClientWithUrl {
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        self.client.send(request)
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        self.client.user_agent()
    }

    fn proxy(&self) -> Option<&Url> {
        self.client.proxy()
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
    handler: FakeHttpHandler,
    user_agent: HeaderValue,
}

#[cfg(any(test, feature = "test"))]
impl FakeHttpClient {
    pub fn create<Fut, F>(handler: F) -> Arc<HttpClientWithUrl>
    where
        Fut: futures::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static,
        F: Fn(Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        Arc::new(HttpClientWithUrl {
            base_url: Mutex::new("http://test.example".into()),
            client: Arc::new(Self {
                handler: Arc::new(move |request| Box::pin(handler(request))),
                user_agent: HeaderValue::from_static(type_name::<Self>()),
            }),
        })
    }

    pub fn with_response(status: StatusCode) -> Arc<HttpClientWithUrl> {
        log::warn!("Using fake HTTP client with {status} response");
        Self::create(move |_| async move {
            let mut response = Response::new(AsyncBody::default());
            *response.status_mut() = status;
            Ok(response)
        })
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
        (self.handler)(request)
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}
