use super::UnixStream;

use crate::reactor::PollEvented;

use futures::task::LocalWaker;
use futures::{ready, Poll, Stream};
use mio_uds;

use std::fmt;
use std::io;
use std::os::unix::io::{AsRawFd, RawFd};
use std::os::unix::net::{self, SocketAddr};
use std::path::Path;
use std::pin::Pin;

/// A Unix socket which can accept connections from other Unix sockets.
///
/// # Examples
///
/// ```no_run
/// #![feature(async_await, await_macro, futures_api)]
/// use romio::uds::{UnixListener, UnixStream};
/// use futures::prelude::*;
///
/// async fn say_hello(mut stream: UnixStream) {
///     await!(stream.write_all(b"Shall I hear more, or shall I speak at this?!"));
/// }
///
/// async fn listen() -> Result<(), Box<dyn std::error::Error + 'static>> {
///     let mut listener = UnixListener::bind("/tmp/sock")?;
///
///     // accept connections and process them serially
///     while let Some(stream) = await!(listener.next()) {
///         await!(say_hello(stream?));
///     }
///     Ok(())
/// }
/// ```
#[must_use = "streams do nothing unless polled"]
pub struct UnixListener {
    io: PollEvented<mio_uds::UnixListener>,
}

impl UnixListener {
    /// Creates a new `UnixListener` bound to the specified path.
    ///
    /// # Examples
    /// Create a Unix Domain Socket on `/tmp/sock`.
    ///
    /// ```rust,no_run
    /// use romio::uds::UnixListener;
    ///
    /// # fn main () -> Result<(), Box<dyn std::error::Error + 'static>> {
    /// let socket = UnixListener::bind("/tmp/sock")?;
    /// # Ok(())}
    /// ```
    ///
    pub fn bind(path: impl AsRef<Path>) -> io::Result<UnixListener> {
        let listener = mio_uds::UnixListener::bind(path)?;
        let io = PollEvented::new(listener);
        Ok(UnixListener { io })
    }

    /// Returns the local socket address of this listener.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use romio::uds::UnixListener;
    ///
    /// # fn main () -> Result<(), Box<dyn std::error::Error + 'static>> {
    /// let socket = UnixListener::bind("/tmp/sock")?;
    /// let addr = socket.local_addr()?;
    /// # Ok(())}
    /// ```
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        self.io.get_ref().local_addr()
    }

    /// Returns the value of the `SO_ERROR` option.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// use romio::uds::UnixListener;
    ///
    /// # fn main () -> Result<(), Box<dyn std::error::Error + 'static>> {
    /// let listener = UnixListener::bind("/tmp/sock")?;
    /// if let Ok(Some(err)) = listener.take_error() {
    ///     println!("Got error: {:?}", err);
    /// }
    /// # Ok(())}
    /// ```
    pub fn take_error(&self) -> io::Result<Option<io::Error>> {
        self.io.get_ref().take_error()
    }

    fn poll_accept(&self, lw: &LocalWaker) -> Poll<io::Result<(UnixStream, SocketAddr)>> {
        let (io, addr) = ready!(self.poll_accept_std(lw)?);

        let io = mio_uds::UnixStream::from_stream(io)?;
        Poll::Ready(Ok((UnixStream::new(io), addr)))
    }

    fn poll_accept_std(&self, lw: &LocalWaker) -> Poll<io::Result<(net::UnixStream, SocketAddr)>> {
        ready!(self.io.poll_read_ready(lw)?);

        match self.io.get_ref().accept_std() {
            Ok(Some((sock, addr))) => Poll::Ready(Ok((sock, addr))),
            Ok(None) => {
                self.io.clear_read_ready(lw)?;
                Poll::Pending
            }
            Err(ref err) if err.kind() == io::ErrorKind::WouldBlock => {
                self.io.clear_read_ready(lw)?;
                Poll::Pending
            }
            Err(err) => Poll::Ready(Err(err)),
        }
    }
}

impl fmt::Debug for UnixListener {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        self.io.get_ref().fmt(f)
    }
}

impl AsRawFd for UnixListener {
    fn as_raw_fd(&self) -> RawFd {
        self.io.get_ref().as_raw_fd()
    }
}

/// An implementation of the `Stream` trait which
/// resolves to the sockets the are accepted on this listener.
///
///
/// # Examples
///
/// ```rust,no_run
/// #![feature(async_await, await_macro, futures_api)]
/// use romio::uds::UnixListener;
/// use futures::prelude::*;
///
/// # async fn run () -> Result<(), Box<dyn std::error::Error + 'static>> {
/// let mut listener = UnixListener::bind("/tmp/sock")?;
///
/// // accept connections and process them serially
/// while let Some(stream) = await!(listener.next()) {
///     match stream {
///         Ok(stream) => {
///             println!("new client!");
///         },
///         Err(e) => { /* connection failed */ }
///     }
/// }
/// # Ok(())}
/// ```
impl Stream for UnixListener {
    type Item = io::Result<UnixStream>;

    fn poll_next(self: Pin<&mut Self>, lw: &LocalWaker) -> Poll<Option<Self::Item>> {
        let (socket, _) = ready!(self.poll_accept(lw)?);
        Poll::Ready(Some(Ok(socket)))
    }
}
