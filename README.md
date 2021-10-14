# actix-tcp-leak
Reproducible [TCP connection leak](https://github.com/actix/actix-net/issues/351) affecting `actix-web`

## Setup

1. Install `netstat` and `libopenssl-devel` (and whatever else is required to compile this)
2. `cargo run`
3. Watch the `CLOSE_WAIT` connections steadily leak.
4. On a laptop on the same network, navigate to `https://IP_OF_RUNNING_PROGRAM:4443`, click through the SSL warning, wait until you see "Hello World!," and then turn off your WiFi to leak one `ESTABLISHED` connection.

## Note

The above is not necessarily limited to Debian 11, that's just what I tested.
