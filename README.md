# actix-tcp-leak
Reproducible TCP connection leak affecting `actix-web`

## Setup

1. Create a VPS with Debian 11 and your SSH public key
2. Add its IP address ot the Makefile
3. Run `make provision-debian-11`
4. Run `ssh debian-11` or otherwise connect to the VPS
5. Run `nohup ./tcp-leak --keep-alive 5 --https -p 443 &` on the VPS
6. ~~Install `wrk` and run `make wrk-debian-11-http` and `make wrk-debian-11-https` in parallel~~
7. Route real connections to the VPS public IP
