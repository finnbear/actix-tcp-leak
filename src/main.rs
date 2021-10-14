use actix_web::{web, App, HttpResponse, HttpServer};
use openssl::ssl::{SslConnector, SslMethod, SslVerifyMode};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};
use std::collections::BTreeMap;
use std::io::{BufReader, Write};
use std::net::TcpStream;
use std::process::Command;
use std::time::Duration;
use std::{io, mem, thread};
use tokio::time::sleep;

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let mut counter = 0;
    thread::spawn(move || loop {
        println!("{}, {:?}", counter, connection_counts());
        counter += 1;
        thread::sleep(Duration::from_secs(1));

        leak();
    });

    let app = move || {
        App::new().service(web::resource("/").route(web::get().to(|| async {
            sleep(Duration::from_secs(2)).await;
            HttpResponse::Ok().body("Hello World!")
        })))
    };

    let mut config = ServerConfig::new(NoClientAuth::new());
    let cert_chain = certs(&mut BufReader::new(&include_bytes!("example.crt")[..])).unwrap();
    let mut keys =
        pkcs8_private_keys(&mut BufReader::new(&include_bytes!("example.key")[..])).unwrap();
    config.set_single_cert(cert_chain, keys.remove(0)).unwrap();

    HttpServer::new(app)
        .bind_rustls("0.0.0.0:4443", config)?
        .run()
        .await
}

fn leak() {
    let stream = TcpStream::connect("127.0.0.1:4443").unwrap();

    let mut builder = SslConnector::builder(SslMethod::tls()).unwrap();
    builder.set_verify(SslVerifyMode::NONE);
    let connector = builder.build();

    let mut stream = connector.connect("localhost", stream).unwrap();
    stream.ssl_write(&b"prepare to die, actix web"[..]).unwrap();
    stream.flush().unwrap();

    // Removing this avoids the leak, but the client cannot be trusted.
    mem::forget(stream);
}

fn connection_counts() -> BTreeMap<String, usize> {
    let mut ret = BTreeMap::new();

    let output = Command::new("netstat").arg("-natp").output().unwrap();
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
