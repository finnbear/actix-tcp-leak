# actix-tcp-leak
Reproducible TCP connection leak affecting `actix-web` + `rustls`

## Setup

1. Install `netstat` and `libssl-dev` (and whatever else is required to compile this)
2. `cargo run`
3. Watch the `CLOSE_WAIT` connections steadily leak.
4. On a laptop on the same network, navigate to `https://IP_OF_RUNNING_PROGRAM:4443`, click through the SSL warning, wait until you see "Hello World!," and then turn off your WiFi to leak one `ESTABLISHED` connection.

## Docker Setup

1. `docker build -t tcp-leak .`
2. `docker run tcp-leak`