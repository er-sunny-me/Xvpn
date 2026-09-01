#!/bin/bash
set -e

echo "Extracting project..."
tar -xzf vpn-lab.tar.gz
cd rust-vpn-lab

echo "Checking for Rust..."
if ! command -v cargo &> /dev/null; then
    echo "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
fi
source $HOME/.cargo/env || true

echo "Installing build dependencies..."
sudo apt-get update -y
sudo apt-get install -y build-essential pkg-config libssl-dev tmux

echo "Building server..."
cargo build --release -p server

echo "Setting up NAT..."
sudo bash tools/setup_nat.sh

echo "Starting server in tmux session 'vpn-server'..."
tmux kill-session -t vpn-server 2>/dev/null || true
tmux new-session -d -s vpn-server 'sudo ./target/release/server --host 0.0.0.0 --port 51820'

echo "Deployment complete! VPN Server is running on port 51820."
