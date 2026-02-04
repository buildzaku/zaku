use std::{
    io::{Cursor, Read},
    pin::Pin,
    task::Poll,
};

use bytes::Bytes;
use futures::AsyncRead;
use http_body::{Body, Frame};

pub struct AsyncBody(pub Inner);

pub enum Inner {
    Empty,
    Bytes(std::io::Cursor<Bytes>),
    AsyncReader(Pin<Box<dyn futures::AsyncRead + Send + Sync>>),
}

impl AsyncBody {
    pub fn empty() -> Self {
        Self(Inner::Empty)
    }

    pub fn from_reader<R>(read: R) -> Self
    where
        R: AsyncRead + Send + Sync + 'static,
    {
        Self(Inner::AsyncReader(Box::pin(read)))
    }

    pub fn from_bytes(bytes: Bytes) -> Self {
        Self(Inner::Bytes(Cursor::new(bytes)))
    }
}

impl Default for AsyncBody {
    fn default() -> Self {
        Self(Inner::Empty)
    }
}

impl From<()> for AsyncBody {
    fn from(_: ()) -> Self {
        Self(Inner::Empty)
    }
}

impl From<Bytes> for AsyncBody {
    fn from(bytes: Bytes) -> Self {
        Self::from_bytes(bytes)
    }
}

impl From<Vec<u8>> for AsyncBody {
    fn from(body: Vec<u8>) -> Self {
        Self::from_bytes(body.into())
    }
}

impl From<String> for AsyncBody {
    fn from(body: String) -> Self {
        Self::from_bytes(body.into())
    }
}

impl From<&'static [u8]> for AsyncBody {
    #[inline]
    fn from(bytes: &'static [u8]) -> Self {
        Self::from_bytes(Bytes::from_static(bytes))
    }
}

impl From<&'static str> for AsyncBody {
    #[inline]
    fn from(value: &'static str) -> Self {
        Self::from_bytes(Bytes::from_static(value.as_bytes()))
    }
}

impl<T: Into<Self>> From<Option<T>> for AsyncBody {
    fn from(body: Option<T>) -> Self {
        match body {
            Some(body) => body.into(),
            None => Self::empty(),
        }
    }
}

impl futures::AsyncRead for AsyncBody {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buffer: &mut [u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        let inner = unsafe { &mut self.get_unchecked_mut().0 };
        match inner {
            Inner::Empty => Poll::Ready(Ok(0)),
            Inner::Bytes(cursor) => Poll::Ready(cursor.read(buffer)),
            Inner::AsyncReader(async_reader) => {
                AsyncRead::poll_read(async_reader.as_mut(), cx, buffer)
            }
        }
    }
}

impl Body for AsyncBody {
    type Data = Bytes;
    type Error = std::io::Error;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let mut buffer = vec![0; 8192];
        match AsyncRead::poll_read(self.as_mut(), cx, &mut buffer) {
            Poll::Ready(Ok(0)) => Poll::Ready(None),
            Poll::Ready(Ok(size)) => {
                let data = Bytes::copy_from_slice(&buffer[..size]);
                Poll::Ready(Some(Ok(Frame::data(data))))
            }
            Poll::Ready(Err(error)) => Poll::Ready(Some(Err(error))),
            Poll::Pending => Poll::Pending,
        }
    }
}
