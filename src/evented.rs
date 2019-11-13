use log::*;
use mio::{unix::EventedFd, PollOpt, Ready, Token};
use std::{io, os::unix::io::RawFd};

#[derive(Debug)]
pub struct Evented(RawFd);

impl Evented {
    pub fn new(fd: RawFd) -> Self {
        Self(fd)
    }
}

impl mio::Evented for Evented {
    fn register(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        trace!("Register ZMQ fd: {}", self.0);
        EventedFd(&self.0).register(poll, token, interest, opts)
    }

    fn reregister(
        &self,
        poll: &mio::Poll,
        token: Token,
        interest: Ready,
        opts: PollOpt,
    ) -> io::Result<()> {
        trace!("Re-register ZMQ fd: {}", self.0);
        EventedFd(&self.0).reregister(poll, token, interest, opts)
    }

    fn deregister(&self, poll: &mio::Poll) -> io::Result<()> {
        trace!("De-register ZMQ fd: {}", self.0);
        EventedFd(&self.0).deregister(poll)
    }
}
