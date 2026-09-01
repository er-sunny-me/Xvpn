<div align="center">
  <h1>🔒 Xvpn</h1>
  <p><strong>A blazingly fast, lightweight, and single-executable VPN written in Rust.</strong></p>
  
  <p>
    <img src="https://img.shields.io/badge/Language-Rust-orange.svg" alt="Rust">
    <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20Linux-blue.svg" alt="Platform">
    <img src="https://img.shields.io/badge/License-MIT-green.svg" alt="License">
  </p>
</div>

## 🚀 Overview

**Xvpn** is a high-performance VPN built completely in Rust. It prioritizes speed, simplicity, and ease of use. Forget about complex configurations, dependencies, or installers. Xvpn ships as a **single, portable `.exe` file** for Windows clients, with an automated 1-click deployment script for Linux servers.

It utilizes raw UDP sockets and the highly efficient Wintun driver to route internet traffic securely and with minimal overhead.

## ✨ Features

- ⚡ **Blazingly Fast:** Written in Rust, leveraging zero-copy packet forwarding and raw UDP sockets for minimal latency.
- 📦 **Standalone Executable:** The Windows client is a single 1.3MB `.exe` file. The Wintun driver is embedded directly into the binary—no installers required.
- 🛡️ **Auto-Elevation:** Automatically requests Administrator privileges (UAC) if required, making it incredibly user-friendly.
- 🌐 **Full-Tunnel Routing:** Intelligently configures Windows routing tables to securely funnel all internet traffic through the VPN without creating routing loops.
- 🛠️ **1-Click Server Deployment:** Includes a PowerShell script (`deploy_server.ps1`) to automatically package, upload, compile, and install the Linux server as a robust `systemd` background service on AWS/VPS.
- 🧹 **Clean Shutdown:** Gracefully handles `Ctrl+C` to instantly clean up routing tables and restore the original network state upon exit.

## 🏗️ Architecture

- **`client/`**: The Windows VPN client (Standalone `.exe`). Connects to the server, creates the virtual network adapter, and updates routes.
- **`server/`**: The Linux VPN server. Listens for incoming UDP packets, forwards traffic through NAT, and handles responses.
- **`shared/`**: Shared protocol definitions (Bincode serialized messaging).

## 💻 Usage

### Server Setup (Linux / AWS)
Just run the deployment script from your Windows machine. It will automatically upload the code to your server, compile it, configure `iptables` for IP forwarding, and set it up as a background `systemd` service.

```powershell
.\deploy_server.ps1
```
*(Make sure to update `$KEY` and `$SERVER` in the script to match your AWS credentials).*

### Client Setup (Windows)
1. Build the standalone client:
```powershell
.\build.bat
```
2. Double-click the generated **`Xvpn.exe`**.
3. Allow Administrator access when prompted.
4. On the first run, the console will ask for your server IP. Enter it, and it will be saved to `server.txt` for all future connections.
5. You are connected! Press `Ctrl+C` in the console window to disconnect and clean up routes.

## 🤝 Contributing

Contributions, issues, and feature requests are welcome! 

## 📝 License

This project is licensed under the [MIT License](LICENSE).
