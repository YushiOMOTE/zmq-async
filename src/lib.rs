mod evented;

use crate::evented::Evented;
use futures::future::poll_fn;
use mio::Ready;
use std::{
    cell::RefCell,
    io,
    task::{Context, Poll, Waker},
};
use tokio::io::PollEvented;

pub use zmq;

pub struct Socket {
    sock: zmq::Socket,
    evented: PollEvented<Evented>,
    read: RefCell<Option<Waker>>,
    write: RefCell<Option<Waker>>,
}

impl Socket {
    /// Create a async socket instance from `zmq::Socket`
    pub async fn new(sock: zmq::Socket) -> io::Result<Self> {
        let evented = PollEvented::new(Evented::new(sock.get_fd()?))?;

        Ok(Self {
            sock,
            evented,
            read: RefCell::new(None),
            write: RefCell::new(None),
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

    /// Send a multi-part message.
    pub async fn send_multipart<T>(&self, msgs: &[T]) -> io::Result<()>
    where
        T: AsRef<[u8]>,
    {
        let msgs: Vec<&[u8]> = msgs.iter().map(|m| m.as_ref()).collect();
        poll_fn(|cx| self.poll_write(cx, &msgs)).await
    }

    /// Receive a multi-part message.
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
    /// Wake up task which is waiting for read
    fn wakeup_read(&self) {
        self.read.borrow().as_ref().map(|w| w.wake_by_ref());
    }

    /// Wake up task which is waiting for write
    fn wakeup_write(&self) {
        self.write.borrow().as_ref().map(|w| w.wake_by_ref());
    }

    fn sleep_read(&self, cx: &Context) {
        self.read.borrow_mut().replace(cx.waker().clone());
    }

    fn sleep_write(&self, cx: &Context) {
        self.write.borrow_mut().replace(cx.waker().clone());
    }

    fn poll_write(&self, cx: &mut Context, msg: &[&[u8]]) -> Poll<io::Result<()>> {
        let events = self.sock.get_events()?;

        if events.intersects(zmq::POLLOUT) {
            match self.sock.send_multipart(msg, zmq::DONTWAIT) {
                Ok(_) => Poll::Ready(Ok(())),
                Err(zmq::Error::EAGAIN) => unreachable!(),
                Err(e) => Poll::Ready(Err(e.into())),
            }
        } else {
            self.evented.clear_write_ready(cx)?;
            if events.intersects(zmq::POLLIN) {
                self.wakeup_read();
            }
            self.sleep_write(cx);
            Poll::Pending
        }
    }

    fn poll_read(&self, cx: &mut Context) -> Poll<io::Result<Vec<Vec<u8>>>> {
        let events = self.sock.get_events()?;

        if events.intersects(zmq::POLLIN) {
            match self.sock.recv_multipart(zmq::DONTWAIT) {
                Ok(msg) => Poll::Ready(Ok(msg)),
                Err(zmq::Error::EAGAIN) => unreachable!(),
                Err(e) => Poll::Ready(Err(e.into())),
            }
        } else {
            self.evented.clear_read_ready(cx, Ready::readable())?;
            if events.intersects(zmq::POLLOUT) {
                self.wakeup_write();
            }
            self.sleep_read(cx);
            Poll::Pending
        }
    }
}
