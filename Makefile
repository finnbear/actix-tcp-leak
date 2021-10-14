DEBIAN_11 = 192.46.220.91
.RECIPEPREFIX +=

provision:
    cargo build --release
    scp target/release/tcp-leak root@$(DEBIAN_11):/root/tcp-leak

ssh:
    ssh root@$(DEBIAN_11)

wrk:
    wrk -t32 -c300 --timeout 30s -d1h https://$(DEBIAN_11):4443