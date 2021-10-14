# actix-tcp-leak
Reproducible [TCP connection leak](https://github.com/actix/actix-net/issues/351) affecting `actix-web`

## Setup

1. Create a VPS with Debian 11 and your SSH public key
2. Add its IP address ot the Makefile
3. Run `make provision`
4. Run `make ssh` or otherwise connect to the VPS
5. Run `./tcp-leak` on the VPS
6. Install `wrk` and run `make wrk`
7. Disconnect your internet connection (I tested on a laptop with WiFi, and turning off the WiFi was sufficient)
8. Observe that some or all of the inflight TCP connections are permanently leaked
9. As an alternative to 6-8, observe that ~1 out of every 100 real-end-user connection to the VPS public IP will leak

## Note

The above is not necessarily limited to Debian 11, that's just what I tested.
