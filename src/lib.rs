mod convert;
mod evented;

use crate::{convert::FromMessage, evented::Evented};
use mio::Ready;
use std::{
    io,
    task::{Context, Poll},
};
use tokio::{future::poll_fn, net::util::PollEvented, sync::AtomicWaker};

pub use zmq;

pub struct Socket {
    sock: zmq::Socket,
    evented: PollEvented<Evented>,
    read: AtomicWaker,
    write: AtomicWaker,
}

/// Helper to clone `zmq::Message` object
///
/// Cloning messages is required before sending because
/// `zmq` crate doesn't give back the message on EAGAIN.
///
/// https://github.com/erickt/rust-zmq/issues/211
///
fn copy_msg(msg: &zmq::Message) -> zmq::Message {
    msg.to_vec().into()
}

impl Socket {
    /// Create a async socket instance from `zmq::Socket`
    pub async fn new(sock: zmq::Socket) -> io::Result<Self> {
        let evented = PollEvented::new(Evented::new(sock.get_fd()?))?;

        Ok(Self {
            sock,
            evented,
            read: AtomicWaker::new(),
            write: AtomicWaker::new(),
        })
    }

    /// Provides reference to the underlying socket object.
    pub fn socket(&self) -> &zmq::Socket {
        &self.sock
    }

    /// Provides mutable reference to the underlying socket object.
    pub fn socket_mut(&mut self) -> &mut zmq::Socket {
        &mut self.sock
    }

    /// Send a message.
    pub async fn send<T>(&self, data: T, flags: i32) -> io::Result<()>
    where
        T: Into<zmq::Message>,
    {
        let msg = data.into();
        poll_fn(|cx| self.poll_write(cx, &msg, flags)).await
    }

    /// Send a multi-part message.
    pub async fn send_multipart<I, T>(&self, msgs: I) -> io::Result<()>
    where
        I: IntoIterator<Item = T>,
        T: Into<zmq::Message>,
    {
        let mut iter = msgs.into_iter().peekable();

        while let Some(m) = iter.next() {
            let flags = if iter.peek().is_some() {
                zmq::SNDMORE
            } else {
                0
            };
            self.send(m, flags).await?;
        }

        Ok(())
    }

    /// Receive a message.
    pub async fn recv(&self) -> io::Result<zmq::Message> {
        self.recv_as().await
    }

    /// Receive a message as a specific primitive type.
    pub async fn recv_as<T>(&self) -> io::Result<T>
    where
        T: FromMessage,
    {
        let msg = poll_fn(|cx| self.poll_read(cx)).await?;
        Ok(FromMessage::from(msg))
    }

    /// Receive a multi-part message.
    pub async fn recv_multipart(&self) -> io::Result<Vec<zmq::Message>> {
        self.recv_multipart_as().await
    }

    /// Receive a multi-part message as a specific primitive type.
    pub async fn recv_multipart_as<T>(&self) -> io::Result<Vec<T>>
    where
        T: FromMessage,
    {
        let mut parts = vec![];
        loop {
            let part = self.recv_as().await?;
            parts.push(part);

            if !self.sock.get_rcvmore()? {
                break;
            }
        }
        Ok(parts)
    }

    /// Check the socket readiness via ZMQ_EVENTS.
    ///
    /// By using this method, the read readiness needs to be checked
    /// after writing messages to sockets (and vice versa).
    ///
    /// Such checking is necessary according to the following note about zeromq file descriptor.
    ///
    /// > The returned file descriptor is also used internally by the zmq_send and
    /// > zmq_recv functions. As the descriptor is edge triggered, applications must
    /// > update the state of ZMQ_EVENTS after each invocation of zmq_send or zmq_recv.
    /// > To be more explicit: after calling zmq_send the socket may become readable
    /// > (and vice versa) without triggering a read event on the file descriptor.
    ///
    /// (From ZMQ_FD section in http://api.zeromq.org/4-1:zmq-getsockopt)
    ///
    fn check_readiness(&self, events: zmq::PollEvents) -> io::Result<bool> {
        let flg = self.sock.get_events()?;
        Ok((flg & events).is_empty())
    }

    /// Wake up task which is waiting for read
    fn wakeup_read(&self) -> io::Result<()> {
        if self.check_readiness(zmq::POLLIN)? {
            self.read.wake();
        }

        Ok(())
    }

    /// Wake up task which is waiting for write
    fn wakeup_write(&self) -> io::Result<()> {
        if self.check_readiness(zmq::POLLOUT)? {
            self.write.wake();
        }

        Ok(())
    }

    fn poll_write(&self, cx: &mut Context, msg: &zmq::Message, flags: i32) -> Poll<io::Result<()>> {
        match self.sock.send(copy_msg(msg), zmq::DONTWAIT | flags) {
            Ok(_) => {
                self.wakeup_read()?;
                Poll::Ready(Ok(()))
            }
            Err(zmq::Error::EAGAIN) => {
                self.evented.clear_read_ready(cx, Ready::readable())?;
                self.write.register(cx.waker().clone());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e.into())),
        }
    }

    fn poll_read(&self, cx: &mut Context) -> Poll<io::Result<zmq::Message>> {
        match self.sock.recv_msg(zmq::DONTWAIT) {
            Ok(msg) => {
                self.wakeup_write()?;
                Poll::Ready(Ok(msg))
            }
            Err(zmq::Error::EAGAIN) => {
                self.evented.clear_write_ready(cx)?;
                self.read.register(cx.waker().clone());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e.into())),
        }
    }
}
