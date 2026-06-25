use anyhow::anyhow;
use bytes::{BufMut, Bytes, BytesMut};
use futures::{FutureExt, TryStreamExt};
use reqwest::redirect;
use std::{io, mem, pin::Pin, sync::OnceLock, task, time::Duration};

use http_client::{AsyncBody, HttpClient, Inner, RedirectPolicy, Url, http};

const DEFAULT_CAPACITY: usize = 4096;
static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();

pub struct ReqwestClient {
    client: reqwest::Client,
    no_redirect_client: reqwest::Client,
    handle: tokio::runtime::Handle,
}

impl ReqwestClient {
    fn builder() -> reqwest::ClientBuilder {
        reqwest::Client::builder()
            .use_rustls_tls()
            .connect_timeout(Duration::from_secs(10))
    }

    pub fn new() -> Self {
        let client = Self::builder()
            .build()
            .expect("Failed to initialize HTTP client");
        let no_redirect_client = Self::builder()
            .redirect(redirect::Policy::none())
            .build()
            .expect("Failed to initialize HTTP client");
        let handle =
            tokio::runtime::Handle::try_current().unwrap_or_else(|_| runtime().handle().clone());
        Self {
            client,
            no_redirect_client,
            handle,
        }
    }
}

impl Default for ReqwestClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient for ReqwestClient {
    fn send(
        &self,
        request: http::Request<AsyncBody>,
    ) -> futures::future::BoxFuture<'static, anyhow::Result<http::Response<AsyncBody>>> {
        let (parts, body) = request.into_parts();
        let redirect_policy = parts
            .extensions
            .get::<RedirectPolicy>()
            .cloned()
            .unwrap_or(RedirectPolicy::FollowAll);
        let follow_limit_client = match redirect_policy {
            RedirectPolicy::FollowLimit(limit) => Some(
                Self::builder()
                    .redirect(redirect::Policy::limited(limit as usize))
                    .build()
                    .expect("Failed to initialize HTTP client"),
            ),
            _ => None,
        };
        let client = match redirect_policy {
            RedirectPolicy::NoFollow => &self.no_redirect_client,
            RedirectPolicy::FollowAll => &self.client,
            RedirectPolicy::FollowLimit(_) => follow_limit_client
                .as_ref()
                .expect("Follow limit client should be initialized"),
        };

        let mut request = client.request(parts.method, parts.uri.to_string());
        request = request.headers(parts.headers);
        let request = request.body(match body.0 {
            Inner::Empty => reqwest::Body::default(),
            Inner::Bytes(cursor) => cursor.into_inner().into(),
            Inner::AsyncReader(reader) => reqwest::Body::wrap_stream(StreamReader::new(reader)),
        });

        let handle = self.handle.clone();
        async move {
            let mut response = handle
                .spawn(async { request.send().await })
                .await?
                .map_err(|error| anyhow!(error))?;

            let headers = mem::take(response.headers_mut());
            let mut builder = http::Response::builder()
                .status(response.status().as_u16())
                .version(response.version());
            *builder
                .headers_mut()
                .expect("Response headers should be available") = headers;

            let bytes = response
                .bytes_stream()
                .map_err(futures::io::Error::other)
                .into_async_read();
            let body = AsyncBody::from_reader(bytes);

            builder.body(body).map_err(|error| anyhow!(error))
        }
        .boxed()
    }

    fn user_agent(&self) -> Option<&http::HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }
}

pub fn runtime() -> &'static tokio::runtime::Runtime {
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("Failed to initialize HTTP client")
    })
}

struct StreamReader {
    reader: Option<Pin<Box<dyn futures::AsyncRead + Send + Sync>>>,
    buffer: BytesMut,
    capacity: usize,
}

impl StreamReader {
    fn new(reader: Pin<Box<dyn futures::AsyncRead + Send + Sync>>) -> Self {
        Self {
            reader: Some(reader),
            buffer: BytesMut::new(),
            capacity: DEFAULT_CAPACITY,
        }
    }
}

impl futures::Stream for StreamReader {
    type Item = io::Result<Bytes>;

    fn poll_next(
        mut self: Pin<&mut Self>,
        cx: &mut task::Context<'_>,
    ) -> task::Poll<Option<Self::Item>> {
        let mut this = self.as_mut();

        let Some(mut reader) = this.reader.take() else {
            return task::Poll::Ready(None);
        };

        if this.buffer.capacity() == 0 {
            let capacity = this.capacity;
            this.buffer.reserve(capacity);
        }

        match poll_read_buffer(reader.as_mut(), cx, &mut this.buffer) {
            task::Poll::Pending => task::Poll::Pending,
            task::Poll::Ready(Err(error)) => {
                self.reader = None;
                task::Poll::Ready(Some(Err(error)))
            }
            task::Poll::Ready(Ok(0)) => {
                self.reader = None;
                task::Poll::Ready(None)
            }
            task::Poll::Ready(Ok(_)) => {
                let chunk = this.buffer.split();
                self.reader = Some(reader);
                task::Poll::Ready(Some(Ok(chunk.freeze())))
            }
        }
    }
}

fn poll_read_buffer<T: futures::AsyncRead + ?Sized, B: BufMut>(
    reader: Pin<&mut T>,
    cx: &mut task::Context<'_>,
    buffer: &mut B,
) -> task::Poll<io::Result<usize>> {
    if !buffer.has_remaining_mut() {
        return task::Poll::Ready(Ok(0));
    }

    let size = {
        let destination = buffer.chunk_mut();

        // SAFETY: `chunk_mut()` returns a `&mut UninitSlice`, and `UninitSlice` is a
        // transparent wrapper around `[MaybeUninit<u8>]`.
        let destination = unsafe { destination.as_uninit_slice_mut() };
        let mut buffer = tokio::io::ReadBuf::uninit(destination);

        let pointer = buffer.filled().as_ptr();
        let unfilled_portion = buffer.initialize_unfilled();
        task::ready!(reader.poll_read(cx, unfilled_portion)?);

        // Ensure the pointer does not change from under us
        assert_eq!(pointer, buffer.filled().as_ptr());
        buffer.filled().len()
    };

    // SAFETY: This is guaranteed to be the number of initialized (and read)
    // bytes due to the invariants provided by `ReadBuf::filled`.
    unsafe {
        buffer.advance_mut(size);
    }

    task::Poll::Ready(Ok(size))
}
