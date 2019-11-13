use structopt::StructOpt;

#[derive(StructOpt)]
struct Opt {
    addr: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let opt = Opt::from_args();

    let ctx = zmq::Context::new();

    let sock = ctx.socket(zmq_async::zmq::SocketType::DEALER)?;
    sock.connect(&opt.addr)?;

    let sock = zmq_async::Socket::new(sock).await?;
    sock.send_multipart(&["hi"]).await?;
    println!("Sent");
    let msgs = sock.recv_multipart_as::<Vec<u8>>().await?;
    println!("Received: {:?}", msgs);
    assert_eq!(vec![b"hi".to_vec()], msgs);

    Ok(())
}
