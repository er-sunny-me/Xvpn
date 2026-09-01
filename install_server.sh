#!/bin/bash
set -e

echo "==========================================="
echo " Installing RustVPN Server"
echo "==========================================="

cd /home/ubuntu/rust-vpn-lab

# 1. Build the server
echo "[1/4] Compiling server (Release Mode)..."
export PATH="$HOME/.cargo/bin:$PATH"
cargo build --release --bin server

# 2. Configure Networking (IP Forwarding)
echo "[2/4] Configuring IP Forwarding..."
sudo bash -c 'echo "net.ipv4.ip_forward = 1" > /etc/sysctl.d/99-rustvpn.conf'
sudo sysctl -p /etc/sysctl.d/99-rustvpn.conf

# 3. Configure NAT (iptables)
echo "[3/4] Configuring NAT (iptables)..."
# Find the default network interface (usually ens5 or eth0)
DEFAULT_IFACE=$(ip route | grep default | awk '{print $5}')

# Clear existing rule if it exists
sudo iptables -t nat -D POSTROUTING -s 10.8.0.0/24 -o $DEFAULT_IFACE -j MASQUERADE 2>/dev/null || true
# Add the new rule
sudo iptables -t nat -A POSTROUTING -s 10.8.0.0/24 -o $DEFAULT_IFACE -j MASQUERADE

# Save iptables rules so they persist across reboots
echo iptables-persistent iptables-persistent/autosave_v4 boolean true | sudo debconf-set-selections
echo iptables-persistent iptables-persistent/autosave_v6 boolean true | sudo debconf-set-selections
sudo apt-get install -y iptables-persistent >/dev/null 2>&1
sudo netfilter-persistent save >/dev/null 2>&1

# 4. Install Systemd Service
echo "[4/4] Installing systemd service..."
sudo cp rustvpn.service /etc/systemd/system/
sudo systemctl daemon-reload
sudo systemctl enable rustvpn.service
sudo systemctl restart rustvpn.service

echo "==========================================="
echo " ✅ Deployment Complete!"
echo " Server is now running in the background."
echo " To check logs: journalctl -u rustvpn -f"
echo "==========================================="
