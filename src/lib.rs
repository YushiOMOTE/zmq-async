mod evented;
mod waker;

use crate::{evented::Evented, waker::TaskWaker};
use mio::Ready;
use std::{
    io,
    ops::{Deref, DerefMut},
    task::{Context, Poll},
};
use tokio::{future::poll_fn, net::util::PollEvented};

pub use zmq;

pub struct Socket {
    sock: zmq::Socket,
    evented: PollEvented<Evented>,
    read: TaskWaker,
    write: TaskWaker,
    readable: bool,
    writable: bool,
}

impl Deref for Socket {
    type Target = zmq::Socket;

    fn deref(&self) -> &Self::Target {
        &self.sock
    }
}

impl DerefMut for Socket {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sock
    }
}

impl Socket {
    pub async fn new(sock: zmq::Socket) -> io::Result<Self> {
        let evented = PollEvented::new(Evented::new(sock.get_fd()?))?;

        let (readable, writable) = get_readwritable(sock.get_socket_type()?);

        Ok(Self {
            sock,
            evented,
            read: TaskWaker::new(),
            write: TaskWaker::new(),
            readable,
            writable,
        })
    }

    pub async fn send_multipart<I, T>(&self, msgs: I) -> io::Result<()>
    where
        I: IntoIterator<Item = T> + Clone,
        T: Into<zmq::Message>,
    {
        poll_fn(|cx| self.poll_write(cx, msgs.clone())).await
    }

    pub async fn recv_multipart(&self) -> io::Result<Vec<Vec<u8>>> {
        poll_fn(|cx| self.poll_read(cx)).await
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
        if self.check_readiness(zmq::POLLIN)? && self.readable {
            self.read.wake();
        }

        Ok(())
    }

    /// Wake up task which is waiting for write
    fn wakeup_write(&self) -> io::Result<()> {
        if self.check_readiness(zmq::POLLOUT)? && self.writable {
            self.write.wake();
        }

        Ok(())
    }

    fn poll_write<I, T>(&self, cx: &mut Context, msgs: I) -> Poll<io::Result<()>>
    where
        I: IntoIterator<Item = T>,
        T: Into<zmq::Message>,
    {
        if !self.writable {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "Socket is not writable",
            )));
        }

        match self.sock.send_multipart(msgs, zmq::DONTWAIT) {
            Ok(_) => {
                self.wakeup_read()?;
                Poll::Ready(Ok(()))
            }
            Err(zmq::Error::EAGAIN) => {
                self.evented.clear_read_ready(cx, Ready::readable())?;
                self.write.register(cx.waker());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e.into())),
        }
    }

    fn poll_read(&self, cx: &mut Context) -> Poll<io::Result<Vec<Vec<u8>>>> {
        if !self.readable {
            return Poll::Ready(Err(io::Error::new(
                io::ErrorKind::Other,
                "Socket is not readable",
            )));
        }

        match self.sock.recv_multipart(zmq::DONTWAIT) {
            Ok(msg) => {
                self.wakeup_write()?;
                Poll::Ready(Ok(msg))
            }
            Err(zmq::Error::EAGAIN) => {
                self.evented.clear_write_ready(cx)?;
                self.read.register(cx.waker());
                Poll::Pending
            }
            Err(e) => Poll::Ready(Err(e.into())),
        }
    }
}

fn get_readwritable(socktype: zmq::SocketType) -> (bool, bool) {
    match socktype {
        zmq::SocketType::PUSH => (false, true),
        zmq::SocketType::PULL => (true, false),
        zmq::SocketType::PUB => (false, true),
        zmq::SocketType::SUB => (true, false),
        zmq::SocketType::XPUB => (true, true),
        zmq::SocketType::XSUB => (true, true),
        zmq::SocketType::REQ => (true, true),
        zmq::SocketType::REP => (true, true),
        zmq::SocketType::ROUTER => (true, true),
        zmq::SocketType::DEALER => (true, true),
        zmq::SocketType::PAIR => (true, true),
        zmq::SocketType::STREAM => (true, true),
    }
}
