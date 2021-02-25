#!/bin/bash
echo "===                       Sample build script                     ==="
echo "= Note that this can be any executable file, like a python script   ="
cargo test
cargo build
# also make musl build
mkdir -p ./tmp
docker build -t thingy:$(BRANCH) .
docker run --rm -it -v "$PWD"/tmp:/tmp thingy:$(BRANCH) sh -c "cp -fv /app/thingy /tmp/"
# distribute/deploy builds?
