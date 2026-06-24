mod async_body;

use futures::future::BoxFuture;
use http::HeaderValue;

#[cfg(feature = "test")]
use parking_lot::Mutex;

#[cfg(feature = "test")]
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

    #[cfg(feature = "test")]
    fn as_fake(&self) -> &FakeHttpClient {
        panic!("as_fake should only be called for FakeHttpClient");
    }
}

#[cfg(feature = "test")]
type FakeHttpHandler = Arc<
    dyn Fn(Request<AsyncBody>) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>>
        + Send
        + Sync
        + 'static,
>;

#[cfg(feature = "test")]
pub struct FakeHttpClient {
    handler: Mutex<Option<FakeHttpHandler>>,
    user_agent: HeaderValue,
}

#[cfg(feature = "test")]
impl FakeHttpClient {
    pub fn create<Fut, F>(handler: F) -> Arc<Self>
    where
        Fut: futures::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static,
        F: Fn(Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        Arc::new(Self {
            handler: Mutex::new(Some(Arc::new(move |request| Box::pin(handler(request))))),
            user_agent: HeaderValue::from_static(type_name::<Self>()),
        })
    }

    pub fn with_404_response() -> Arc<Self> {
        log::warn!("Using fake HTTP client with 404 response");
        Self::create(|_| async move {
            Ok(Response::builder()
                .status(404)
                .body(AsyncBody::default())
                .unwrap())
        })
    }

    pub fn with_200_response() -> Arc<Self> {
        log::warn!("Using fake HTTP client with 200 response");
        Self::create(|_| async move {
            Ok(Response::builder()
                .status(200)
                .body(AsyncBody::default())
                .unwrap())
        })
    }

    pub fn replace_handler<Fut, F>(&self, new_handler: F)
    where
        Fut: futures::Future<Output = anyhow::Result<Response<AsyncBody>>> + Send + 'static,
        F: Fn(FakeHttpHandler, Request<AsyncBody>) -> Fut + Send + Sync + 'static,
    {
        let mut handler = self.handler.lock();
        let old_handler = handler.take().unwrap();
        *handler = Some(Arc::new(move |request| {
            Box::pin(new_handler(old_handler.clone(), request))
        }));
    }
}

#[cfg(feature = "test")]
impl fmt::Debug for FakeHttpClient {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FakeHttpClient").finish()
    }
}

#[cfg(feature = "test")]
impl HttpClient for FakeHttpClient {
    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, anyhow::Result<Response<AsyncBody>>> {
        self.handler.lock().as_ref().unwrap()(request)
    }

    fn user_agent(&self) -> Option<&HeaderValue> {
        Some(&self.user_agent)
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn as_fake(&self) -> &FakeHttpClient {
        self
    }
}
