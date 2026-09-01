<div align="center">
  <h1>🔒 Xvpn</h1>
  <p><strong>A blazingly fast, lightweight, and standalone VPN written in Rust.</strong></p>
  
  <p>
    <img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust">
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-blue.svg" alt="Platform">
    <img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License">
  </p>
</div>

## 🚀 Overview

**Xvpn** is a high-performance VPN built completely in Rust. It prioritizes speed, simplicity, and ease of use. Forget about complex configurations, dependencies, or installers. 

The Windows client compiles to a **single, portable `.exe` file** (with an embedded Wintun driver), and the Linux server comes with a fully automated 1-click deployment script.

## 📁 What's in the Repository?

- **`client/`**: The Rust source code for the Windows VPN client.
- **`server/`**: The Rust source code for the Linux VPN server.
- **`shared/`**: Shared protocol definitions (Bincode serialized messaging) used by both.
- **`deploy_server.ps1`**: A PowerShell script to push code to your server and deploy it automatically.
- **`install_server.sh`**: The bash script that runs on the server to install dependencies and configure the firewall.
- **`build.bat`**: A simple script to compile the Windows client executable.
- **`rustvpn.service`**: The systemd service definition for the Linux server.

---

## 🛠️ Step-by-Step Setup Guide

### 1. Server Setup (AWS / Linux VPS)

> [!CAUTION]
> **CRITICAL FIREWALL REQUIREMENT**: Before deploying, you **MUST** configure your AWS Security Group (or VPS Firewall) to allow inbound **UDP traffic on Port 51820**. If you skip this, the client will time out and fail to connect.

1. Open `deploy_server.ps1` in a text editor.
2. Update the `$KEY` variable with the path to your SSH private key (e.g., `C:\keys\mykey.pem`).
3. Update the `$SERVER` variable with your server's username and IP (e.g., `ubuntu@13.235.x.x`).
4. Run the script from Windows PowerShell:
   ```powershell
   .\deploy_server.ps1
   ```
This script will upload the code, compile the server, enable IP forwarding, configure NAT (`iptables`), and install Xvpn as a background `systemd` service that starts automatically on boot.

### 2. Client Setup (Windows)

Because we don't include compiled `.exe` files in the source code, you must build it yourself the first time:

1. Ensure you have [Rust](https://rustup.rs/) installed on Windows.
2. Run the build script:
   ```powershell
   .\build.bat
   ```
3. This will create a standalone **`Xvpn.exe`** file in the root folder.
4. Double-click **`Xvpn.exe`**.
5. **First Run:** A console window will pop up asking for your Server IP. Type the IP address of your AWS/VPS server and press Enter. It will be saved to a `server.txt` file.
6. **Allow Admin:** Click "Yes" when Windows asks for Administrator privileges (required to create the virtual network adapter).
7. You are connected! To disconnect, just press `Ctrl+C` in the console window.

---

## 🚑 Troubleshooting & Common Problems

Here are the most common issues users face and how to fix them:

> [!WARNING]
> **Error: "Server not responding (timeout)"**
> - **Cause:** The client sent a connection request to the server, but the server never replied.
> - **Solution:** Check your AWS Security Group / Firewall. Ensure **Inbound UDP Port 51820** is open. Also, ensure you typed the correct Server IP.

> [!IMPORTANT]
> **Error: "Failed to elevate: Admin privileges required"**
> - **Cause:** You clicked "No" on the User Account Control (UAC) prompt, or ran it from a restricted shell.
> - **Solution:** Xvpn must run as Administrator to create the `Wintun` virtual network adapter and modify Windows routing tables. Run it again and click "Yes".

> [!NOTE]
> **Problem: "I entered the wrong IP on the first run and now it's stuck!"**
> - **Cause:** The client remembered the wrong IP.
> - **Solution:** Open the folder containing `Xvpn.exe`. You will see a file named `server.txt`. Open it and change the IP, or simply delete the file to make the client prompt you again.

> [!WARNING]
> **Problem: "It says Connected, but my internet doesn't work!"**
> - **Cause:** The VPN tunnel is established, but your Linux server is not routing traffic to the internet.
> - **Solution:** SSH into your Linux server and verify IP forwarding is enabled by running `cat /proc/sys/net/ipv4/ip_forward` (it should output `1`). If not, the `install_server.sh` script may have failed. 

## 📝 License

This project is licensed under the [MIT License](LICENSE).
