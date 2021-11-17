#![allow(unused_imports)]

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, Recipient, StreamHandler};
use actix_server::Server;
use actix_service::{fn_service, ServiceFactoryExt as _};
use actix_web::web::BytesMut;
use actix_web::{web, App, HttpRequest, HttpResponse, HttpServer};
use actix_web_actors::ws;
use core::task::Poll::Ready;
use futures_util::future::ok;
use futures_util::task::noop_waker;
use h2::client;
use http::{Method, Request};
use lazy_static::lazy_static;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use x509_parser::pem::parse_x509_pem;
use rustls_pemfile;
use rustls::server::{NoClientAuth, ServerConfig};
use std::collections::BTreeMap;
use std::collections::HashSet;
use std::error::Error;
use std::future::Future;
use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::os::unix::prelude::AsRawFd;
use std::pin::Pin;
use std::process::Command;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use std::{io, mem, thread};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;
use websocket::ClientBuilder;

// If true, go directly to actix-server. However, as TCP keep alives are not implemented, the
// connection leak is arguably to be expected in this case.
const USE_ACTIX_SERVER: bool = false;

lazy_static! {
    static ref FIREHOSES: Arc<Mutex<HashSet<Recipient<Water>>>> =
        Arc::new(Mutex::new(HashSet::new()));
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let mut counter = 0;
    thread::spawn(move || {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            loop {
                println!("{}, {:?}", counter, connection_counts());
                counter += 1;

                thread::sleep(Duration::from_millis(100));

                if counter == 21 {
                    println!("No more connections will be leaked to avoid crashing. Monitor the existing connections to see if they ever close.")
                } else if counter < 21 {
                    // There are many ways to leak connections :)
                    if USE_ACTIX_SERVER {
                        // Note: One of the leaked connections is a client->server (irrelevant) but the other
                        // is server->client. However, due to a lack of TCP keep alive, it is arguably to be expected
                        // that the connections leak here.
                        leak_one_close_wait_socket_or_two_established_sockets_if_actix_server();
                    } else {
                        // Note: One of the leaked connections is a client->server (irrelevant) but the other
                        // is server->client.
                        //leak_two_established_socket_tls();

                        leak_two_established_websocket();

                        /*
                        leak_two_established_socket_http2_handshake();
                        leak_two_established_socket_http2_keepalive();
                         */
                    }
                }
            }
        });
    });

    thread::spawn(|| loop {
        thread::sleep(Duration::from_millis(100));
        let mut err_count = 0;
        let mut total = 0;
        for firehose in FIREHOSES.lock().unwrap().iter() {
            if firehose.do_send(Water).is_err() {
                err_count += 1;
            }
            total += 1;
        }

        if err_count > 0 {
            println!("{} out of {} do_send(Water) failed.", err_count, total);
        }
    });

    if USE_ACTIX_SERVER {
        // TODO: Does actix_server offer a keepalive option?
        Server::build()
            .bind("echo", ("0.0.0.0", 1080), move || {
                fn_service(move |mut stream: actix_rt::net::TcpStream| {
                    async move {
                        let mut size = 0;
                        let mut buf = BytesMut::new();

                        loop {
                            match stream.read_buf(&mut buf).await {
                                // end of stream; bail from loop
                                Ok(0) => break,

                                // more bytes to process
                                Ok(bytes_read) => {
                                    //println!("read {} bytes", bytes_read);
                                    stream.write_all(&buf[size..]).await.unwrap();
                                    size += bytes_read;
                                }

                                // stream error; bail from loop with error
                                Err(err) => {
                                    println!("Stream Error: {:?}", err);
                                    return Err(());
                                }
                            }
                        }

                        // send data down service pipeline
                        Ok((buf.freeze(), size))
                    }
                })
                .map_err(|err| println!("Service Error: {:?}", err))
                .and_then(move |(_, size)| ok(size))
            })?
            .workers(1)
            .run()
            .await
    } else {
        let app = move || {
            App::new()
                .service(web::resource("/").route(web::get().to(|| async {
                    sleep(Duration::from_secs(5)).await;
                    HttpResponse::Ok().body("Hello World!")
                })))
                .service(web::resource("/firehose").route(web::get().to(index)))
        };

        let mut cert_reader = &mut BufReader::new(&include_bytes!("example.crt")[..]);
        let mut priv_reader = &mut BufReader::new(&include_bytes!("example.key")[..]);

        let config = ServerConfig::builder()
            .with_safe_defaults()
            .with_client_cert_verifier(NoClientAuth::new())
            .with_single_cert(
                rustls_pemfile::certs(&mut cert_reader)
                    .unwrap()
                    .into_iter()
                    .map(|v| rustls::Certificate(v))
                    .collect(),
                rustls::PrivateKey(
                    rustls_pemfile::pkcs8_private_keys(&mut priv_reader)
                        .unwrap()
                        .into_iter()
                        .next()
                        .unwrap(),
                ),
            )
            .unwrap();

        HttpServer::new(app)
            .keep_alive(1)
            .bind_rustls("0.0.0.0:1443", config)?
            .bind("0.0.0.0:1080")?
            .run()
            .await
    }
}

struct Firehose {
    last_activity: Instant,
}

impl Actor for Firehose {
    type Context = ws::WebsocketContext<Self>;

    fn started(&mut self, ctx: &mut Self::Context) {
        ctx.run_interval(Duration::from_secs(3), |act, ctx| {
            if act.last_activity.elapsed() > Duration::from_secs(10) {
                ctx.close(None);
                ctx.stop();
                println!("Stopping WS...");
            } else {
                ctx.ping(b"");
                println!("Pinging WS...");
            }
        });

        println!("A firehose is starting");
        FIREHOSES.lock().unwrap().insert(ctx.address().recipient());
        println!("A firehose started");
    }

    fn stopped(&mut self, ctx: &mut Self::Context) {
        println!("A firehose is stopping");
        FIREHOSES.lock().unwrap().remove(&ctx.address().recipient());
        println!("A firehose stopped");
    }
}

impl StreamHandler<Result<ws::Message, ws::ProtocolError>> for Firehose {
    fn handle(&mut self, _msg: Result<ws::Message, ws::ProtocolError>, _ctx: &mut Self::Context) {
        self.last_activity = Instant::now();
        // can just ignore incoming messages
    }
}

#[derive(Message)]
#[rtype(result = "()")]
struct Water;

impl Handler<Water> for Firehose {
    type Result = ();

    fn handle(&mut self, _: Water, ctx: &mut Self::Context) -> Self::Result {
        // Enough to fill the network interface on my computer.
        for _ in 0..1000 {
            ctx.text("BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES \
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES\
        BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES BYTES");
        }
    }
}

async fn index(req: HttpRequest, stream: web::Payload) -> Result<HttpResponse, actix_web::Error> {
    ws::start(
        Firehose {
            last_activity: Instant::now(),
        },
        &req,
        stream,
    )
}

fn leak_two_established_websocket() {
    let client = ClientBuilder::new("http://localhost:1080/firehose")
        .unwrap()
        .add_protocol("rust-websocket")
        .connect_insecure()
        .unwrap();

    mem::forget(client);
}

fn disable_keep_alives<S: AsRawFd>(stream: &S) {
    let fd = stream.as_raw_fd();
    unsafe {
        let flag = 0;
        libc::setsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_KEEPALIVE,
            mem::transmute(&flag as *const i32),
            mem::size_of_val(&flag) as u32,
        );
    }
}

#[allow(dead_code)]
fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    let waker = noop_waker();
    let mut context = Context::from_waker(&waker);
    Future::poll(future, &mut context)
}

/// Leaks one client->server socket (irrelevant) and one server->client socket.
#[allow(dead_code)]
fn leak_two_established_socket_tls() {
    let stream = TcpStream::connect("127.0.0.1:1443").unwrap();

    // NOTE: SSL port but no SSL handshake.

    disable_keep_alives(&stream);

    // Removing this avoids the leak, but the client cannot be trusted.
    mem::forget(stream);
}

/*
fn leak_two_established_socket_http2_handshake() {
    tokio::spawn(async {
        match tokio::net::TcpStream::connect("127.0.0.1:1080").await {
            Ok(tcp) => {
                disable_keep_alives(&tcp);

                let mut fut = Box::pin(client::handshake(tcp));
                if let Ready(_) = poll_once(Pin::as_mut(&mut fut)) {
                    //panic!("handshake actually completed");
                }

                //drop(tcp);
                mem::forget(fut);
            }
            Err(e) => println!("{:?}", e)
        }
    });
}

pub fn normal_http2() {
    tokio::spawn(async {
        let _ = async {
            // Establish TCP connection to the server.
            let tcp = tokio::net::TcpStream::connect("127.0.0.1:1080").await?;
            let (h2, connection) = client::handshake(tcp).await?;
            tokio::spawn(async move {
                connection.await.unwrap();
            });

            let mut h2 = h2.ready().await?;
            // Prepare the HTTP request to send to the server.
            let request = Request::builder()
                .method(Method::GET)
                .uri("https://www.example.com/")
                .body(())
                .unwrap();

            // Send the request. The second tuple item allows the caller
            // to stream a request body.
            let (response, _) = h2.send_request(request, true).unwrap();

            let (head, mut body) = response.await?.into_parts();

            println!("Received response: {:?}", head);

            // The `flow_control` handle allows the caller to manage
            // flow control.
            //
            // Whenever data is received, the caller is responsible for
            // releasing capacity back to the server once it has freed
            // the data from memory.
            let mut flow_control = body.flow_control().clone();

            while let Some(chunk) = body.data().await {
                let chunk = chunk?;
                println!("RX: {:?}", chunk);

                // Let the server send more data.
                let _ = flow_control.release_capacity(chunk.len());
            }

            Result::<(), Box<dyn Error>>::Ok(())
        }.await;
    });
}

fn leak_two_established_socket_http2_keepalive() {
    tokio::spawn(async {
        match tokio::net::TcpStream::connect("127.0.0.1:1080").await {
            Ok(tcp) => {
                //disable_keep_alives(&tcp);
                match client::handshake(tcp).await {
                    Ok((h2, mut connection)) => {
                        let request = Request::builder()
                            .method(Method::GET)
                            .uri("http://127.0.0.1/")
                            .body(())
                            .unwrap();

                        let mut h2 = h2.ready().await.unwrap();

                        let (response, _) = h2.send_request(request, true).unwrap();

                        let _ = response.await;

                        mem::forget(connection.ping_pong());
                        let mut fut = Box::pin(connection);
                        for _ in 0..500 {
                            if let Ready(_) = poll_once(Pin::as_mut(&mut fut)) {
                                panic!("connection actually completed");
                            }
                        }

                        mem::forget(fut);
                    }
                    Err(e) => println!("{:?}", e)
                }
            }
            Err(e) => println!("{:?}", e)
        }
    });
}
 */

/// Returns the count of each connection state on the process, including both client and server
/// sockets.
fn connection_counts() -> BTreeMap<String, usize> {
    let mut ret = BTreeMap::new();

    let output = Command::new("netstat")
        .arg("-natp")
        .output()
        .expect("net-tools must be installed");
    for s in std::str::from_utf8(&output.stdout)
        .unwrap()
        .split('\n')
        .filter(|s| {
            s.contains("tcp-leak") || s.contains("target/debug") || s.contains("target/release")
        })
    {
        let mut key = None;
        for potential in [
            "LISTEN",
            "SYN_SENT",
            "SYN_RECEIVED",
            "ESTABLISHED",
            "FIN_WAIT_1",
            "FIN_WAIT_2",
            "CLOSE_WAIT",
            "CLOSING",
            "LAST_ACK",
            "TIME_WAIT",
            "CLOSED",
        ] {
            if s.contains(potential) {
                key = Some(potential);
                break;
            }
        }

        if let Some(key) = key {
            *ret.entry(key.to_owned()).or_insert(0) += 1;
        }
    }

    ret
}

// Any case in which only one connection leaks may be a case in which the server acts properly and the
// client is the one that leaks. These cases may require further investigation.
#[allow(dead_code)]
fn leak_one_close_wait_socket_or_two_established_sockets_if_actix_server() {
    let mut stream = TcpStream::connect("127.0.0.1:1080").unwrap();

    stream.write(&b"this is ok"[..]).unwrap();
    stream.flush().unwrap();

    disable_keep_alives(&stream);

    mem::forget(stream);
}

// See above comment.
#[allow(dead_code)]
fn leak_one_close_wait_socket_tls() {
    let stream = TcpStream::connect("127.0.0.1:1443").unwrap();

    disable_keep_alives(&stream);

    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    builder.set_verify(SslVerifyMode::NONE);
    let connector = builder.build();

    let mut stream = connector.connect("localhost", stream).unwrap();
    stream.ssl_write(&b"prepare to die, actix web"[..]).unwrap();
    stream.flush().unwrap();

    // Removing this avoids the leak, but the client cannot be trusted.
    mem::forget(stream);
}
