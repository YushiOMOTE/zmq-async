#[test]
fn echo() {
    let mut rt = tokio::runtime::Runtime::new().unwrap();

    let ctx = zmq::Context::new();
    let ctx2 = ctx.clone();

    std::thread::spawn(move || {
        let mut rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let srv = {
                let sock = ctx2.socket(zmq_async::zmq::SocketType::ROUTER).unwrap();
                sock.bind("inproc://channel").unwrap();
                zmq_async::Socket::new(sock).await.unwrap()
            };

            loop {
                let msgs = srv.recv_multipart().await.unwrap();
                srv.send_multipart(msgs).await.unwrap();
            }
        });
    });

    rt.block_on(async {
        let cli = {
            let sock = ctx.socket(zmq_async::zmq::SocketType::DEALER).unwrap();
            sock.connect("inproc://channel").unwrap();
            zmq_async::Socket::new(sock).await.unwrap()
        };

        cli.send_multipart(&["hi"]).await.unwrap();
        assert_eq!(vec![b"hi".to_vec()], cli.recv_multipart().await.unwrap());
    });
}
