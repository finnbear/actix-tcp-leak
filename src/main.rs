use actix_server::Server;
use actix_service::{fn_service, ServiceFactoryExt as _};
use actix_web::web::BytesMut;
use actix_web::{web, App, HttpResponse, HttpServer};
use futures_util::future::ok;
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};
use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::os::unix::prelude::AsRawFd;
use std::process::Command;
use std::time::Duration;
use std::{io, mem, thread};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;

// If true, go directly to actix-server. However, as TCP keep alives are not implemented, the
// connection leak is arguably to be expected in this case.
const USE_ACTIX_SERVER: bool = false;

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let mut counter = 0;
    thread::spawn(move || loop {
        println!("{}, {:?}", counter, connection_counts());
        counter += 1;

        thread::sleep(Duration::from_secs(1));

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
                leak_two_established_socket_tls();
            }
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
            App::new().service(web::resource("/").route(web::get().to(|| async {
                sleep(Duration::from_secs(5)).await;
                HttpResponse::Ok().body("Hello World!")
            })))
        };

        let mut config = ServerConfig::new(NoClientAuth::new());
        let cert_chain = certs(&mut BufReader::new(&include_bytes!("example.crt")[..])).unwrap();
        let mut keys =
            pkcs8_private_keys(&mut BufReader::new(&include_bytes!("example.key")[..])).unwrap();
        config.set_single_cert(cert_chain, keys.remove(0)).unwrap();

        HttpServer::new(app)
            .keep_alive(1)
            .bind_rustls("0.0.0.0:1443", config)?
            .bind("0.0.0.0:1080")?
            .run()
            .await
    }
}

fn disable_keep_alives(stream: &TcpStream) {
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

/// Leaks one client->server socket (irrelevant) and one server->client socket.
fn leak_two_established_socket_tls() {
    let stream = TcpStream::connect("127.0.0.1:1443").unwrap();

    // NOTE: SSL port but no SSL handshake.

    disable_keep_alives(&stream);

    // Removing this avoids the leak, but the client cannot be trusted.
    mem::forget(stream);
}

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
