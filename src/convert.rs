pub trait FromMessage {
    fn from(msg: zmq::Message) -> Self;
}

impl FromMessage for zmq::Message {
    fn from(msg: zmq::Message) -> Self {
        msg
    }
}

impl FromMessage for String {
    fn from(msg: zmq::Message) -> Self {
        String::from_utf8_lossy(&msg).to_string()
    }
}

impl FromMessage for Vec<u8> {
    fn from(msg: zmq::Message) -> Self {
        msg.to_vec()
    }
}

impl FromMessage for Box<[u8]> {
    fn from(msg: zmq::Message) -> Self {
        (*msg).into()
    }
}
