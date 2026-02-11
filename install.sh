#!/bin/sh
set -eu
cargo build --release
cp target/release/hopper /usr/local/bin/hopper
cp hopper.path /etc/systemd/system/hopper.path
cp hopper.service /etc/systemd/system/hopper.service
systemctl daemon-reload
systemctl enable --now hopper.path
