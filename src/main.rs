#![feature(async_closure)]

use actix_http::KeepAlive;
use actix_web::{web, App, HttpResponse, HttpServer};
use rustls::internal::pemfile::{certs, pkcs8_private_keys};
use rustls::{NoClientAuth, ServerConfig};
use std::io::BufReader;
use std::process::Command;
use std::time::Duration;
use std::{io, thread};
use tokio::time::sleep;

#[actix_rt::main]
async fn main() -> io::Result<()> {
    let mut counter = 0;
    thread::spawn(move || loop {
        println!("{}, {}", counter, established_connection_count());
        counter += 1;
        thread::sleep(Duration::from_secs(1));
    });

    let app = move || {
        App::new().service(web::resource("/").route(web::get().to(async || {
            sleep(Duration::from_secs(1)).await;
            HttpResponse::Ok().body("Hello World!")
        })))
    };

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

    let mut config = ServerConfig::new(NoClientAuth::new());
    let cert_chain = certs(&mut BufReader::new(&include_bytes!("example.crt")[..])).unwrap();
    let mut keys =
        pkcs8_private_keys(&mut BufReader::new(&include_bytes!("example.key")[..])).unwrap();
    config.set_single_cert(cert_chain, keys.remove(0)).unwrap();

    HttpServer::new(app)
        .keep_alive(KeepAlive::Timeout(5))
        .bind_rustls("0.0.0.0:443", config)?
        .run()
        .await
}

fn established_connection_count() -> usize {
    let output = Command::new("netstat").arg("-natp").output().unwrap();
    std::str::from_utf8(&output.stdout)
        .unwrap()
        .split('\n')
        .filter(|s| s.contains("ESTABLISHED") && s.contains("tcp-leak"))
        .count()
}
