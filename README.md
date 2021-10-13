# actix-tcp-leak
Reproducible TCP connection leak affecting `actix-web`

## Setup

1. Create a VPS with Debian 11 and your SSH public key
2. Add its IP address ot the Makefile
3. Run `make provision-debian-11`
4. Run `make ssh-debian-11` or otherwise connect to the VPS
5. Run `nohup ./tcp-leak --keep-alive 5 --https -p 443 &` on the VPS
6. Install `wrk` and run `make wrk-debian-11-http` OR `make wrk-debian-11-https`
7. Disconnect your internet connection (I tested on a laptop with WiFi, and turning off the WiFi was sufficient)
8. Observe that some or all of the inflight TCP connections are permanently leaked
9. As an alternative to 6-8, observe that ~1 out of every 100 real-end-user connection to the VPS public IP will leak
