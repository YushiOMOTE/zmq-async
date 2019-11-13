use structopt::StructOpt;

#[derive(StructOpt)]
struct Opt {
    addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let ctx = zmq::Context::new();

    let sock = ctx.socket(zmq_async::zmq::SocketType::ROUTER)?;
    sock.bind(&opt.addr)?;
    let sock = zmq_async::Socket::new(sock).await?;

    loop {
        let msgs = sock.recv_multipart().await?;
        println!("Received: {:?}", msgs);
        sock.send_multipart(msgs).await?;
        println!("Sent");
    }
}
