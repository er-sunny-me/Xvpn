use clap::{Parser, Subcommand};
use shared::Message;
use std::env;
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::io::{self, Write};
use serde::Serialize;

// Embed wintun.dll directly into the binary
const WINTUN_DLL: &[u8] = include_bytes!("../wintun.dll");

// Default server configuration
const DEFAULT_SERVER: &str = "your-server-ip-or-domain.com";
const DEFAULT_PORT: u16 = 51820;
const DEFAULT_TUN_IP: &str = "10.8.0.2";
const DEFAULT_TUN_NETMASK: &str = "255.255.255.0";
const DEFAULT_TUN_GATEWAY: &str = "10.8.0.1";

#[derive(Parser)]
#[command(name = "Xvpn", version = "1.0.0", about = "A fast, lightweight VPN client")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Connect to the VPN server (default action)
    Connect {
        #[arg(long, default_value = DEFAULT_SERVER)]
        server: String,
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value = DEFAULT_TUN_IP)]
        tun_ip: String,
        #[arg(long, default_value = DEFAULT_TUN_NETMASK)]
        tun_netmask: String,
    },
    /// Run a UDP benchmark against the server
    Benchmark {
        #[arg(default_value = "udp")]
        protocol: String,
        #[arg(long, default_value = DEFAULT_SERVER)]
        host: String,
        #[arg(long, default_value_t = DEFAULT_PORT)]
        port: u16,
        #[arg(long, default_value_t = 10000)]
        packets: u64,
        #[arg(long, default_value_t = 64)]
        payload: usize,
    },
}

#[derive(Serialize)]
struct BenchmarkResult {
    test: String,
    packets: u64,
    payload_bytes: usize,
    min_ms: f64,
    avg_ms: f64,
    median_ms: f64,
    p95_ms: f64,
    p99_ms: f64,
    max_ms: f64,
    jitter_ms: f64,
    packet_loss_percent: f64,
    packets_per_sec: f64,
}

// ─── Pretty Console Helpers ──────────────────────────────────────

fn print_banner() {
    println!();
    println!("  ╔══════════════════════════════════════╗");
    println!("  ║         \x1b[36m🔒 Xvpn Client v1.0\x1b[0m          ║");
    println!("  ╚══════════════════════════════════════╝");
    println!();
}

fn print_step(step: u8, total: u8, msg: &str) {
    print!("  [{}/{}] {}...", step, total, msg);
}

fn print_ok() {
    println!(" \x1b[32m✓\x1b[0m");
}

fn print_fail(err: &str) {
    println!(" \x1b[31m✗\x1b[0m");
    eprintln!("       \x1b[31mError: {}\x1b[0m", err);
}

// ─── Admin Elevation ─────────────────────────────────────────────

fn is_elevated() -> bool {
    // Try a quick admin-only operation
    let output = Command::new("net").args(["session"]).output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn elevate_self() {
    let exe = env::current_exe().expect("Failed to get current exe path");
    let args: Vec<String> = env::args().skip(1).collect();
    let args_str = args.join(" ");

    let exe_str = exe.to_string_lossy();

    // Use PowerShell to trigger UAC elevation
    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-Command",
            &format!(
                "Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs -Wait",
                exe_str, args_str
            ),
        ])
        .status();

    match status {
        Ok(s) => std::process::exit(s.code().unwrap_or(0)),
        Err(e) => {
            eprintln!("  \x1b[31mFailed to elevate: {}\x1b[0m", e);
            eprintln!("  Please right-click and 'Run as Administrator'");
            std::process::exit(1);
        }
    }
}

// ─── Wintun DLL Extraction ──────────────────────────────────────

fn extract_wintun_dll() -> String {
    let temp_dir = env::temp_dir();
    let dll_path = temp_dir.join("rustvpn_wintun.dll");

    // Only extract if not already present or size differs
    let needs_extract = if dll_path.exists() {
        match fs::metadata(&dll_path) {
            Ok(meta) => meta.len() != WINTUN_DLL.len() as u64,
            Err(_) => true,
        }
    } else {
        true
    };

    if needs_extract {
        fs::write(&dll_path, WINTUN_DLL).expect("Failed to extract wintun.dll to temp directory");
    }

    dll_path.to_string_lossy().to_string()
}

// ─── Route Cleanup ──────────────────────────────────────────────

fn cleanup_routes(server_ip: &str, gateway: &str) {
    println!("\n  \x1b[33mDisconnecting...\x1b[0m");

    // Delete VPN routes
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "delete", "route", "0.0.0.0/1", "interface=Xvpn"])
        .output();
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "delete", "route", "128.0.0.0/1", "interface=Xvpn"])
        .output();

    // Delete server-specific route
    if !server_ip.is_empty() {
        let _ = Command::new("route")
            .args(["delete", server_ip])
            .output();
    }

    // Delete persistent routes if any
    let _ = Command::new("route")
        .args(["delete", "0.0.0.0", "mask", "128.0.0.0"])
        .output();
    let _ = Command::new("route")
        .args(["delete", "128.0.0.0", "mask", "128.0.0.0"])
        .output();

    let _ = gateway; // Used to prevent unused warning

    println!("  \x1b[32mRoutes cleaned up.\x1b[0m");
    println!("  \x1b[32mVPN Disconnected. Your internet is back to normal.\x1b[0m");
}

// ─── Main ────────────────────────────────────────────────────────

fn main() {
    // Enable ANSI colors on Windows
    let _ = enable_virtual_terminal();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Benchmark {
            protocol,
            host,
            port,
            packets,
            payload,
        }) => {
            if protocol == "udp" {
                run_udp_benchmark(&host, port, packets, payload);
            } else {
                eprintln!("Unsupported protocol: {}", protocol);
            }
        }
        Some(Commands::Connect {
            server,
            port,
            tun_ip,
            tun_netmask,
        }) => {
            print_banner();
            ensure_admin();
            let final_server = resolve_server(&server);
            run_tunnel(&final_server, port, &tun_ip, &tun_netmask);
        }
        None => {
            // Default action: connect with defaults
            print_banner();
            ensure_admin();
            let final_server = resolve_server(DEFAULT_SERVER);
            run_tunnel(&final_server, DEFAULT_PORT, DEFAULT_TUN_IP, DEFAULT_TUN_NETMASK);
        }
    }
}

fn resolve_server(provided: &str) -> String {
    let config_file = env::current_dir().unwrap_or_default().join("server.txt");

    // If the user explicitly passed a custom server via CLI, use it (and don't save to txt)
    if provided != DEFAULT_SERVER {
        return provided.to_string();
    }

    // If server.txt exists, read it
    if config_file.exists() {
        if let Ok(contents) = fs::read_to_string(&config_file) {
            let ip = contents.trim().to_string();
            if !ip.is_empty() {
                println!("  \x1b[36mUsing server from server.txt:\x1b[0m {}\n", ip);
                return ip;
            }
        }
    }

    // Otherwise, prompt the user
    println!("  \x1b[33mServer IP not found in server.txt.\x1b[0m");
    print!("  Enter your VPN server IP or domain: ");
    io::stdout().flush().unwrap();

    let mut input = String::new();
    io::stdin().read_line(&mut input).unwrap();
    let ip = input.trim().to_string();

    if ip.is_empty() {
        eprintln!("  \x1b[31mError: Server IP cannot be empty.\x1b[0m");
        std::process::exit(1);
    }

    // Save it for next time
    if let Err(e) = fs::write(&config_file, &ip) {
        eprintln!("  \x1b[33mWarning: Failed to save to server.txt: {}\x1b[0m\n", e);
    } else {
        println!("  \x1b[32mSaved to server.txt!\x1b[0m\n");
    }

    ip
}

fn ensure_admin() {
    if !is_elevated() {
        println!("  \x1b[33mAdmin privileges required. Requesting elevation...\x1b[0m\n");
        elevate_self();
    }
}

fn enable_virtual_terminal() -> Result<(), ()> {
    // Enable ANSI escape codes in Windows console
    let _ = Command::new("cmd").args(["/c", "echo."]).output();
    Ok(())
}

// ─── VPN Tunnel ──────────────────────────────────────────────────

fn run_tunnel(server: &str, port: u16, tun_ip: &str, tun_netmask: &str) {
    let total_steps = 5;

    // Step 1: Extract and load Wintun
    print_step(1, total_steps, "Loading Wintun driver");
    let dll_path = extract_wintun_dll();
    let wintun = unsafe { wintun::load_from_path(&dll_path) };
    let wintun = match wintun {
        Ok(w) => { print_ok(); w }
        Err(e) => { print_fail(&format!("{}", e)); std::process::exit(1); }
    };

    // Step 2: Create adapter
    print_step(2, total_steps, "Creating VPN adapter");
    let adapter = match wintun::Adapter::create(&wintun, "Xvpn", "Xvpn", None) {
        Ok(a) => a,
        Err(_) => match wintun::Adapter::open(&wintun, "Xvpn") {
            Ok(a) => a,
            Err(e) => { print_fail(&format!("{}", e)); std::process::exit(1); }
        },
    };
    print_ok();

    // Give Windows time to initialize the adapter
    thread::sleep(Duration::from_secs(2));

    // Step 3: Configure network
    print_step(3, total_steps, "Configuring network");
    let output = Command::new("netsh")
        .args([
            "interface", "ipv4", "set", "address",
            "name=Xvpn", "source=static",
            tun_ip, tun_netmask, "none",
        ])
        .output()
        .expect("Failed to run netsh");

    if !output.status.success() {
        print_fail(&String::from_utf8_lossy(&output.stderr).to_string());
    } else {
        print_ok();
    }

    let session = Arc::new(
        adapter
            .start_session(wintun::MAX_RING_CAPACITY)
            .expect("Failed to start Wintun session"),
    );

    // Wait for interface to be UP
    thread::sleep(Duration::from_secs(2));

    // Step 4: Connect to server
    print_step(4, total_steps, "Connecting to server");
    let server_addr_str = format!("{}:{}", server, port);
    let server_addr: SocketAddr = match server_addr_str.to_socket_addrs() {
        Ok(mut addrs) => match addrs.find(|a| a.is_ipv4()) {
            Some(a) => a,
            None => { print_fail("No IPv4 address found for server"); std::process::exit(1); }
        },
        Err(e) => { print_fail(&format!("DNS resolution failed: {}", e)); std::process::exit(1); }
    };

    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").expect("Failed to bind UDP socket"));

    // Send initial ping
    let ping_msg = Message::Ping { seq: 0, payload: vec![] };
    let encoded_ping = bincode::serialize(&ping_msg).unwrap();
    if let Err(e) = socket.send_to(&encoded_ping, server_addr) {
        print_fail(&format!("Failed to reach server: {}", e));
        std::process::exit(1);
    }

    // Wait for Pong with timeout
    socket.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut ping_buf = [0u8; 65535];
    match socket.recv_from(&mut ping_buf) {
        Ok((size, _)) => {
            if let Ok(Message::Pong { .. }) = bincode::deserialize::<Message>(&ping_buf[..size]) {
                print_ok();
            } else {
                print_fail("Invalid response from server");
                std::process::exit(1);
            }
        }
        Err(_) => {
            print_fail("Server not responding (timeout)");
            std::process::exit(1);
        }
    }
    // Remove read timeout for the main loop
    socket.set_read_timeout(None).ok();

    // Step 5: Set up routes
    print_step(5, total_steps, "Setting up routes");

    // Set DNS
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "set", "dnsservers", "name=Xvpn", "source=static", "address=8.8.8.8", "register=primary"])
        .output();

    // Find current default gateway
    let route_output = Command::new("route")
        .args(["print", "-4", "0.0.0.0"])
        .output()
        .expect("Failed to run route print");
    let route_stdout = String::from_utf8_lossy(&route_output.stdout);
    let mut gateway = String::new();
    for line in route_stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "0.0.0.0" && parts[1] == "0.0.0.0" {
            gateway = parts[2].to_string();
            break;
        }
    }

    let server_ip = server_addr.ip().to_string();

    // Route server IP through original gateway (prevent routing loop)
    if !gateway.is_empty() {
        let _ = Command::new("route")
            .args(["add", &server_ip, "mask", "255.255.255.255", &gateway])
            .output();
    }

    // Route all traffic through VPN (using netsh for explicit interface binding)
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "add", "route", "0.0.0.0/1", "interface=Xvpn", &format!("nexthop={}", DEFAULT_TUN_GATEWAY), "metric=5"])
        .output();
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "add", "route", "128.0.0.0/1", "interface=Xvpn", &format!("nexthop={}", DEFAULT_TUN_GATEWAY), "metric=5"])
        .output();

    print_ok();

    // ─── Connected! ──────────────────────────────────────────────
    println!();
    println!("  \x1b[32m✅ VPN Connected!\x1b[0m Your traffic is now routed through the VPN.");
    println!("     Server: \x1b[36m{}\x1b[0m", server);
    println!("     VPN IP: \x1b[36m{}\x1b[0m", tun_ip);
    println!();
    println!("  Press \x1b[33mCtrl+C\x1b[0m to disconnect...");
    println!();

    // ─── Ctrl+C Handler ──────────────────────────────────────────
    let running = Arc::new(AtomicBool::new(true));
    let r = running.clone();
    let cleanup_server_ip = server_ip.clone();
    let cleanup_gateway = gateway.clone();

    ctrlc::set_handler(move || {
        cleanup_routes(&cleanup_server_ip, &cleanup_gateway);
        r.store(false, Ordering::SeqCst);
        std::process::exit(0);
    })
    .expect("Error setting Ctrl-C handler");

    // ─── Packet Forwarding ───────────────────────────────────────
    let session_reader = Arc::clone(&session);
    let socket_writer = Arc::clone(&socket);

    // Thread: Read from Wintun → Send to Server via UDP
    thread::spawn(move || {
        loop {
            match session_reader.receive_blocking() {
                Ok(packet) => {
                    let bytes = packet.bytes();
                    let msg = Message::IpPacket {
                        payload: bytes.to_vec(),
                    };
                    if let Ok(encoded) = bincode::serialize(&msg) {
                        let _ = socket_writer.send_to(&encoded, server_addr);
                    }
                }
                Err(_) => break,
            }
        }
    });

    // Main Thread: Read from UDP → Write to Wintun
    let mut buf = [0u8; 65535];
    while running.load(Ordering::SeqCst) {
        match socket.recv_from(&mut buf) {
            Ok((size, _)) => {
                if let Ok(msg) = bincode::deserialize::<Message>(&buf[..size]) {
                    match msg {
                        Message::IpPacket { payload } => {
                            if let Ok(mut packet) =
                                session.allocate_send_packet(payload.len() as u16)
                            {
                                packet.bytes_mut().copy_from_slice(&payload);
                                session.send_packet(packet);
                            }
                        }
                        Message::Pong { .. } => {} // Ignore stray pongs
                        _ => {}
                    }
                }
            }
            Err(_) => {}
        }
    }
}

// ─── Benchmark ───────────────────────────────────────────────────

fn run_udp_benchmark(host: &str, port: u16, packets: u64, payload_size: usize) {
    let addr_str = format!("{}:{}", host, port);
    let addr = addr_str
        .to_socket_addrs()
        .expect("Failed to resolve server address")
        .find(|a| a.is_ipv4())
        .expect("No IPv4 address found for server");
    let socket = UdpSocket::bind("0.0.0.0:0").expect("Failed to bind socket");
    socket
        .set_read_timeout(Some(Duration::from_millis(100)))
        .expect("Failed to set read timeout");

    let payload_data = vec![0u8; payload_size];
    let mut rtts = Vec::with_capacity(packets as usize);
    let mut lost = 0;

    let mut buf = [0u8; 65535];
    let test_start = Instant::now();

    for seq in 0..packets {
        let msg = Message::Ping {
            seq,
            payload: payload_data.clone(),
        };
        let encoded = bincode::serialize(&msg).unwrap();

        let send_time = Instant::now();
        if socket.send_to(&encoded, addr).is_err() {
            lost += 1;
            continue;
        }

        match socket.recv_from(&mut buf) {
            Ok((size, _)) => {
                if let Ok(Message::Pong {
                    seq: recv_seq, ..
                }) = bincode::deserialize::<Message>(&buf[..size])
                {
                    if recv_seq == seq {
                        let rtt = send_time.elapsed();
                        rtts.push(rtt.as_secs_f64() * 1000.0);
                    } else {
                        lost += 1;
                    }
                } else {
                    lost += 1;
                }
            }
            Err(_) => {
                lost += 1;
            }
        }
    }

    let test_duration = test_start.elapsed();

    if rtts.is_empty() {
        eprintln!("All packets lost or server unreachable.");
        return;
    }

    let mut sorted_rtts = rtts.clone();
    sorted_rtts.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min_ms = sorted_rtts[0];
    let max_ms = sorted_rtts[sorted_rtts.len() - 1];
    let sum_ms: f64 = sorted_rtts.iter().sum();
    let avg_ms = sum_ms / sorted_rtts.len() as f64;
    let median_ms = sorted_rtts[sorted_rtts.len() / 2];

    let p95_idx = (sorted_rtts.len() as f64 * 0.95) as usize;
    let p99_idx = (sorted_rtts.len() as f64 * 0.99) as usize;
    let p95_ms = sorted_rtts[p95_idx.min(sorted_rtts.len() - 1)];
    let p99_ms = sorted_rtts[p99_idx.min(sorted_rtts.len() - 1)];

    let mut jitter_sum = 0.0;
    for i in 1..rtts.len() {
        jitter_sum += (rtts[i] - rtts[i - 1]).abs();
    }
    let jitter_ms = if rtts.len() > 1 {
        jitter_sum / (rtts.len() - 1) as f64
    } else {
        0.0
    };

    let packet_loss_percent = (lost as f64 / packets as f64) * 100.0;
    let packets_per_sec = packets as f64 / test_duration.as_secs_f64();

    let result = BenchmarkResult {
        test: "udp_rtt".to_string(),
        packets,
        payload_bytes: payload_size,
        min_ms,
        avg_ms,
        median_ms,
        p95_ms,
        p99_ms,
        max_ms,
        jitter_ms,
        packet_loss_percent,
        packets_per_sec,
    };

    let json = serde_json::to_string_pretty(&result).unwrap();
    println!("{}", json);

    let _ = fs::create_dir_all("results");
    let result_path = format!(
        "results/udp_benchmark_{}.json",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    );
    fs::write(&result_path, &json).unwrap();
    println!("Saved to {}", result_path);
}
