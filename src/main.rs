#![feature(async_closure)]

use std::{io, thread};

use actix_http::KeepAlive;
use actix_web::{web, App, HttpResponse, HttpServer};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};
use std::io::BufReader;
use std::process::Command;
use std::time::Duration;
use structopt::StructOpt;
use tokio::time::sleep;

#[derive(StructOpt)]
struct Options {
    #[structopt(short, long, default_value = "80")]
    port: u16,
    #[structopt(short, long)]
    keep_alive: Option<usize>,
    #[structopt(short, long, default_value = "256")]
    backlog: u32,
    #[structopt(long, default_value = "5000")]
    client_timeout: u64,
    #[structopt(long, default_value = "10")]
    shutdown_timeout: u64,
    #[structopt(long, default_value = "512")]
    max_connections: usize,
    #[structopt(long, default_value = "64")]
    max_connection_rate: usize,
    #[structopt(long)]
    https: bool,
}

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let app = move || {
        App::new().service(web::resource("/").route(web::get().to(async || {
            sleep(Duration::from_secs(1)).await;
            HttpResponse::Ok().body("Hello World!")
        })))
    };

    let options = Options::from_args();

    let mut counter = 0;
    thread::spawn(move || loop {
        let output = Command::new("netstat").arg("-natp").output().unwrap();
        let count = std::str::from_utf8(&output.stdout)
            .unwrap()
            .split('\n')
            .filter(|s| s.contains("ESTABLISHED") && s.contains("tcp-leak"))
            .count();
        println!("{}, {}", counter, count);
        counter += 1;
        thread::sleep(Duration::from_secs(1));
    });

    // Create configuration
    let mut config = ServerConfig::new(NoClientAuth::new());

    let cert_chain = certs(&mut BufReader::new(&include_bytes!("example.crt")[..])).unwrap();
    let mut keys =
        pkcs8_private_keys(&mut BufReader::new(&include_bytes!("example.key")[..])).unwrap();
    config.set_single_cert(cert_chain, keys.remove(0)).unwrap();

    let server = HttpServer::new(app)
        .keep_alive(match options.keep_alive {
            None => KeepAlive::Disabled,
            Some(0) => KeepAlive::Os,
            Some(n) => KeepAlive::Timeout(n),
        })
        .backlog(options.backlog)
        .client_timeout(options.client_timeout)
        .shutdown_timeout(options.shutdown_timeout)
        .max_connections(options.max_connections)
        .max_connection_rate(options.max_connection_rate);

    if options.https {
        server
            .bind_rustls("0.0.0.0:443", config)?
            .bind("0.0.0.0:80")?
            .run()
            .await
    } else {
        server
            .bind(&format!("0.0.0.0:{}", options.port))?
            .run()
            .await
    }
}
