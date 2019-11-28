use clap::arg_enum;
use structopt::StructOpt;

arg_enum! {
    #[derive(Debug, Clone, Copy)]
    enum SocketType {
        PUSH,
        PULL,
        PUB,
        SUB,
        XPUB,
        XSUB,
        REQ,
        REP,
        ROUTER,
        DEALER,
        PAIR,
        STREAM,
    }
}

arg_enum! {
    #[derive(Debug, Clone, Copy)]
    enum Format {
        Debug,
        String,
        Json,
        JsonPretty,
        MsgPack,
        MsgPackPretty,
    }
}

impl From<SocketType> for zmq_async::zmq::SocketType {
    fn from(s: SocketType) -> Self {
        match s {
            SocketType::PUSH => zmq_async::zmq::SocketType::PUSH,
            SocketType::PULL => zmq_async::zmq::SocketType::PULL,
            SocketType::PUB => zmq_async::zmq::SocketType::PUB,
            SocketType::SUB => zmq_async::zmq::SocketType::SUB,
            SocketType::XPUB => zmq_async::zmq::SocketType::XPUB,
            SocketType::XSUB => zmq_async::zmq::SocketType::XSUB,
            SocketType::REQ => zmq_async::zmq::SocketType::REQ,
            SocketType::REP => zmq_async::zmq::SocketType::REP,
            SocketType::ROUTER => zmq_async::zmq::SocketType::ROUTER,
            SocketType::DEALER => zmq_async::zmq::SocketType::DEALER,
            SocketType::PAIR => zmq_async::zmq::SocketType::PAIR,
            SocketType::STREAM => zmq_async::zmq::SocketType::STREAM,
        }
    }
}

#[derive(StructOpt)]
struct SocketOption {
    /// Set to bind instead of connect.
    #[structopt(short = "b", long = "bind")]
    bind: bool,
    /// Set the value of SNDHWM socket option.
    #[structopt(short = "s", long = "sndhwm")]
    sndhwm: Option<i32>,
    /// Set the value of RCVHWM socket option.
    #[structopt(short = "r", long = "rcvhwm")]
    rcvhwm: Option<i32>,
    /// Socket type to use.
    #[structopt(
        name = "socket_type",
        possible_values = &SocketType::variants(),
        case_insensitive = true
    )]
    sock: SocketType,
    /// Address to listen/connect
    #[structopt(name = "addr")]
    addr: String,
}

#[derive(StructOpt)]
enum Mode {
    /// Send messages.
    Send {
        #[structopt(flatten)]
        cfg: SocketOption,
    },
    /// Receive messages.
    Recv {
        #[structopt(flatten)]
        cfg: SocketOption,
    },
    /// Run as echo server.
    EchoServer {
        #[structopt(flatten)]
        cfg: SocketOption,
    },
    /// Run as echo client.
    EchoClient {
        /// The number of messages to send at once.
        #[structopt(short = "d", long = "depth", default_value = "1")]
        depth: usize,
        #[structopt(flatten)]
        cfg: SocketOption,
    },
    /// Dump message body in a specific format.
    Dump {
        /// Socket type to use.
        #[structopt(
            name = "format",
            possible_values = &Format::variants(),
            case_insensitive = true
        )]
        fmt: Format,
        #[structopt(flatten)]
        cfg: SocketOption,
    },
}

#[derive(StructOpt)]
struct Opt {
    /// Samples to measure performance.
    #[structopt(short = "n", long = "samples", default_value = "1000000")]
    samples: usize,
    /// Operations to perform.
    #[structopt(subcommand)]
    mode: Mode,
}

struct Perf {
    samples: usize,
    count: usize,
    total: usize,
    time: Option<std::time::Instant>,
}

impl Perf {
    fn new(samples: usize) -> Self {
        Self {
            samples,
            count: 0,
            total: 0,
            time: None,
        }
    }

    fn rate(&mut self) {
        self.count += 1;
        self.total += 1;

        if self.count >= self.samples {
            let now = std::time::Instant::now();

            if let Some(prev) = self.time.take() {
                let diff = (now - prev).as_millis();
                let kcount = self.count as u128 * 1000;
                let rate = if diff != 0 { kcount / diff } else { 0 };
                println!(
                    "{} items/sec ({} items/{} msec; total {} msgs)",
                    rate, self.count, diff, self.total
                );
            }

            self.count = 0;
            self.time = Some(now);
        }
    }
}

fn setup_sock(ctx: &zmq::Context, cfg: &SocketOption) -> zmq::Result<zmq::Socket> {
    let sock = ctx.socket(cfg.sock.into())?;
    if let Some(sndhwm) = cfg.sndhwm {
        sock.set_sndhwm(sndhwm)?;
    }
    if let Some(rcvhwm) = cfg.rcvhwm {
        sock.set_rcvhwm(rcvhwm)?;
    }
    if cfg.bind {
        sock.bind(&cfg.addr)?;
    } else {
        sock.connect(&cfg.addr)?;
    }
    Ok(sock)
}

#[tokio::main(basic_scheduler)]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let ctx = zmq::Context::new();

    let mut perf = Perf::new(opt.samples);

    match opt.mode {
        Mode::Send { cfg } => {
            println!("Running as sender");

            let sock = setup_sock(&ctx, &cfg)?;
            let sock = zmq_async::Socket::new(sock).await?;

            let mut i = 0u8;
            loop {
                i = i.wrapping_add(1);
                sock.send_multipart(&vec![vec![i]; 1]).await?;
                perf.rate();
            }
        }
        Mode::Recv { cfg } => {
            println!("Running as receiver");

            let sock = setup_sock(&ctx, &cfg)?;
            let sock = zmq_async::Socket::new(sock).await?;

            let mut i = 0u8;
            loop {
                i = i.wrapping_add(1);
                let msg = sock.recv_multipart().await?;
                assert_eq!(msg, vec![vec![i]; 1]);
                perf.rate();
            }
        }
        Mode::EchoServer { cfg } => {
            println!("Running as echo server");

            let sock = setup_sock(&ctx, &cfg)?;
            let sock = zmq_async::Socket::new(sock).await?;

            loop {
                let msg = sock.recv_multipart().await?;
                sock.send_multipart(&msg).await?;
                perf.rate();
            }
        }
        Mode::EchoClient { cfg, depth } => {
            println!("Running as echo client");

            let sock = setup_sock(&ctx, &cfg)?;
            let sock = zmq_async::Socket::new(sock).await?;

            let mut i = 0u8;
            loop {
                let mut sends = vec![];
                for _ in 0..depth {
                    i = i.wrapping_add(1);
                    let p = vec![vec![i]; 1];
                    sock.send_multipart(&p).await?;
                    sends.push(p);
                    perf.rate();
                }
                let mut recvs = vec![];
                for _ in 0..depth {
                    let q = sock.recv_multipart().await?;
                    recvs.push(q);
                    perf.rate();
                }
                assert_eq!(recvs, sends);
            }
        }
        Mode::Dump { cfg, fmt } => {
            println!("Running as receiver");

            let sock = setup_sock(&ctx, &cfg)?;
            let sock = zmq_async::Socket::new(sock).await?;

            loop {
                let msg = sock.recv_multipart().await?;
                match fmt {
                    Format::Debug => {
                        println!("{:?}", msg);
                    }
                    Format::String => {
                        for m in msg {
                            println!("{}", String::from_utf8_lossy(&m));
                        }
                    }
                    Format::Json => {
                        for m in msg {
                            let v: serde_json::Value = serde_json::from_slice(&m)?;
                            println!("{}", serde_json::to_string(&v)?);
                        }
                    }
                    Format::JsonPretty => {
                        for m in msg {
                            let v: serde_json::Value = serde_json::from_slice(&m)?;
                            println!("{}", serde_json::to_string_pretty(&v)?);
                        }
                    }
                    Format::MsgPack => {
                        for m in msg {
                            let v: serde_json::Value = rmp_serde::from_slice(&m)?;
                            println!("{}", serde_json::to_string(&v)?);
                        }
                    }
                    Format::MsgPackPretty => {
                        for m in msg {
                            let v: serde_json::Value = rmp_serde::from_slice(&m)?;
                            println!("{}", serde_json::to_string_pretty(&v)?);
                        }
                    }
                }
            }
        }
    }
}
