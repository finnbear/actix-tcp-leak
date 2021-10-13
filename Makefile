DEBIAN_11 = 192.46.220.91
.RECIPEPREFIX +=

provision-debian-11:
    cargo build --release
    scp target/release/tcp-leak root@$(DEBIAN_11):/root/tcp-leak

ssh-debian-11:
    ssh root@$(DEBIAN_11)

wrk-debian-11-http:
    wrk -t2 -c300 --timeout 1s -d1h http://$(DEBIAN_11)

wrk-debian-11-https:
    wrk -t32 -c300 --timeout 30s -d1h https://$(DEBIAN_11)