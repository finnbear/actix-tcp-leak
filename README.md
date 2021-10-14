# actix-tcp-leak
Reproducible TCP connection leak affecting `actix-web` and `rustls`. In summary, if a client opens a socket
but doesn't attempt (or complete) an SSL handshake, the socket remains open indefinitely.

## Setup

1. Install `netstat` (and whatever else is required to compile this)
2. `cargo run`
3. Watch the `ESTABLISHED` and `CLOSE_WAIT` connections steadily leak.
4. On a laptop on the same network, navigate to `https://IP_OF_RUNNING_PROGRAM:1443`, click through the SSL warning, wait until you see "Hello World!," and then turn off your WiFi to leak one `ESTABLISHED` connection.

## Docker Setup

1. `docker build -t tcp-leak .` or `make docker-build`
2. `docker run tcp-leak` or `make docker-run`

## Expected Result

```console
0, {"LISTEN": 2}
1, {"ESTABLISHED": 2, "LISTEN": 2}
2, {"ESTABLISHED": 4, "LISTEN": 2}
3, {"ESTABLISHED": 6, "LISTEN": 2}
4, {"ESTABLISHED": 8, "LISTEN": 2}
5, {"ESTABLISHED": 10, "LISTEN": 2}
6, {"ESTABLISHED": 12, "LISTEN": 2}
7, {"ESTABLISHED": 14, "LISTEN": 2}
8, {"ESTABLISHED": 16, "LISTEN": 2}
...
thread '<unnamed>' panicked at 'called `Result::unwrap()` on an `Err` value: Os { code: 24, kind: Uncategorized, message: "Too many open files" }'
```