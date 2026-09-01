#![windows_subsystem = "windows"]

use std::env;
use std::fs;
use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use shared::Message;
use tauri::Window;

const WINTUN_DLL: &[u8] = include_bytes!("../../wintun.dll");
const DEFAULT_PORT: u16 = 51820;
const DEFAULT_TUN_IP: &str = "10.8.0.2";
const DEFAULT_TUN_NETMASK: &str = "255.255.255.0";
const DEFAULT_TUN_GATEWAY: &str = "10.8.0.1";

struct VpnState {
    running: Arc<AtomicBool>,
    server_ip: String,
    gateway: String,
}

static mut VPN_STATE: Option<VpnState> = None;

// --- Helper Functions ---

fn log_ui(window: &Window, msg: &str) {
    window.emit("vpn-log", msg).unwrap_or(());
}

fn extract_wintun_dll() -> String {
    let temp_dir = env::temp_dir();
    let dll_path = temp_dir.join("rustvpn_wintun.dll");
    
    let needs_extract = if dll_path.exists() {
        match fs::metadata(&dll_path) {
            Ok(meta) => meta.len() != WINTUN_DLL.len() as u64,
            Err(_) => true,
        }
    } else {
        true
    };
    if needs_extract {
        fs::write(&dll_path, WINTUN_DLL).expect("Failed to extract wintun");
    }
    dll_path.to_string_lossy().to_string()
}

fn cleanup_routes(server_ip: &str, gateway: &str) {
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "delete", "route", "0.0.0.0/1", "interface=Xvpn"])
        .output();
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "delete", "route", "128.0.0.0/1", "interface=Xvpn"])
        .output();
    if !server_ip.is_empty() {
        let _ = Command::new("route").args(["delete", server_ip]).output();
    }
    let _ = Command::new("route").args(["delete", "0.0.0.0", "mask", "128.0.0.0"]).output();
    let _ = Command::new("route").args(["delete", "128.0.0.0", "mask", "128.0.0.0"]).output();
    let _ = gateway;
}

// --- Admin Elevation ---

fn is_elevated() -> bool {
    let output = Command::new("net").args(["session"]).output();
    match output {
        Ok(o) => o.status.success(),
        Err(_) => false,
    }
}

fn elevate_self() {
    let exe = env::current_exe().expect("Failed to get exe path");
    let args: Vec<String> = env::args().skip(1).collect();
    let args_str = args.join(" ");

    let exe_str = exe.to_string_lossy();

    let mut ps_command = format!("Start-Process -FilePath '{}' -Verb RunAs", exe_str);
    if !args_str.is_empty() {
        ps_command = format!("Start-Process -FilePath '{}' -ArgumentList '{}' -Verb RunAs", exe_str, args_str);
    }

    let status = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle", "Hidden",
            "-Command",
            &ps_command,
        ])
        .status();

    match status {
        Ok(_) => std::process::exit(0),
        Err(_) => std::process::exit(1),
    }
}

// --- Tauri Commands ---

#[tauri::command]
fn get_saved_ip() -> String {
    let config_file = env::current_dir().unwrap_or_default().join("server.txt");
    if config_file.exists() {
        if let Ok(contents) = fs::read_to_string(&config_file) {
            return contents.trim().to_string();
        }
    }
    "".to_string()
}

#[tauri::command]
async fn connect_vpn(window: Window, ip: String) -> Result<(), String> {
    log_ui(&window, "Saving IP...");
    let config_file = env::current_dir().unwrap_or_default().join("server.txt");
    let _ = fs::write(&config_file, &ip);

    log_ui(&window, "Loading Wintun driver...");
    let dll_path = extract_wintun_dll();
    let wintun = unsafe { wintun::load_from_path(&dll_path) }
        .map_err(|e| format!("Wintun load failed: {}", e))?;

    log_ui(&window, "Creating virtual adapter...");
    let adapter = wintun::Adapter::create(&wintun, "Xvpn", "Xvpn", None)
        .or_else(|_| wintun::Adapter::open(&wintun, "Xvpn"))
        .map_err(|e| format!("Adapter error: {}", e))?;

    thread::sleep(Duration::from_secs(2));

    log_ui(&window, "Configuring IP address...");
    let output = Command::new("netsh")
        .args(["interface", "ipv4", "set", "address", "name=Xvpn", "source=static", DEFAULT_TUN_IP, DEFAULT_TUN_NETMASK, "none"])
        .output().map_err(|e| e.to_string())?;
        
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).to_string());
    }

    let session = Arc::new(
        adapter.start_session(wintun::MAX_RING_CAPACITY)
            .map_err(|e| format!("Session start failed: {}", e))?
    );

    thread::sleep(Duration::from_secs(2));

    log_ui(&window, "Connecting to server...");
    let server_addr_str = format!("{}:{}", ip, DEFAULT_PORT);
    let server_addr: SocketAddr = server_addr_str.to_socket_addrs()
        .map_err(|e| e.to_string())?
        .find(|a| a.is_ipv4())
        .ok_or("No IPv4 found")?;

    let socket = Arc::new(UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?);
    
    let ping_msg = Message::Ping { seq: 0, payload: vec![] };
    let encoded_ping = bincode::serialize(&ping_msg).unwrap();
    socket.send_to(&encoded_ping, server_addr).map_err(|e| e.to_string())?;
    
    socket.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let mut ping_buf = [0u8; 65535];
    match socket.recv_from(&mut ping_buf) {
        Ok((size, _)) => {
            if bincode::deserialize::<Message>(&ping_buf[..size]).is_err() {
                return Err("Invalid response from server".into());
            }
        }
        Err(_) => return Err("Server not responding (timeout)".into())
    }
    socket.set_read_timeout(None).ok();

    log_ui(&window, "Setting up routes...");
    let _ = Command::new("netsh")
        .args(["interface", "ipv4", "set", "dnsservers", "name=Xvpn", "source=static", "address=8.8.8.8", "register=primary"])
        .output();

    let route_output = Command::new("route").args(["print", "-4", "0.0.0.0"]).output().unwrap();
    let route_stdout = String::from_utf8_lossy(&route_output.stdout);
    let mut gateway = String::new();
    for line in route_stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 3 && parts[0] == "0.0.0.0" && parts[1] == "0.0.0.0" {
            gateway = parts[2].to_string();
            break;
        }
    }

    let server_ip_str = server_addr.ip().to_string();
    if !gateway.is_empty() {
        let _ = Command::new("route").args(["add", &server_ip_str, "mask", "255.255.255.255", &gateway]).output();
    }
    
    let _ = Command::new("netsh").args(["interface", "ipv4", "add", "route", "0.0.0.0/1", "interface=Xvpn", &format!("nexthop={}", DEFAULT_TUN_GATEWAY), "metric=5"]).output();
    let _ = Command::new("netsh").args(["interface", "ipv4", "add", "route", "128.0.0.0/1", "interface=Xvpn", &format!("nexthop={}", DEFAULT_TUN_GATEWAY), "metric=5"]).output();

    let running = Arc::new(AtomicBool::new(true));
    
    unsafe {
        VPN_STATE = Some(VpnState {
            running: running.clone(),
            server_ip: server_ip_str.clone(),
            gateway: gateway.clone(),
        });
    }

    let session_reader = Arc::clone(&session);
    let socket_writer = Arc::clone(&socket);
    
    thread::spawn(move || {
        loop {
            match session_reader.receive_blocking() {
                Ok(packet) => {
                    let bytes = packet.bytes();
                    let msg = Message::IpPacket { payload: bytes.to_vec() };
                    if let Ok(encoded) = bincode::serialize(&msg) {
                        let _ = socket_writer.send_to(&encoded, server_addr);
                    }
                }
                Err(_) => break,
            }
        }
    });

    let running_loop = running.clone();
    thread::spawn(move || {
        let mut buf = [0u8; 65535];
        while running_loop.load(Ordering::SeqCst) {
            if let Ok((size, _)) = socket.recv_from(&mut buf) {
                if let Ok(msg) = bincode::deserialize::<Message>(&buf[..size]) {
                    if let Message::IpPacket { payload } = msg {
                        if let Ok(mut packet) = session.allocate_send_packet(payload.len() as u16) {
                            packet.bytes_mut().copy_from_slice(&payload);
                            session.send_packet(packet);
                        }
                    }
                }
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn disconnect_vpn() -> Result<(), String> {
    unsafe {
        if let Some(state) = &VPN_STATE {
            state.running.store(false, Ordering::SeqCst);
            cleanup_routes(&state.server_ip, &state.gateway);
            VPN_STATE = None;
        }
    }
    Ok(())
}

fn main() {
    if !is_elevated() {
        elevate_self();
        return;
    }

    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_saved_ip,
            connect_vpn,
            disconnect_vpn
        ])
        .on_window_event(|event| match event.event() {
            tauri::WindowEvent::CloseRequested { .. } => {
                // Ensure routes are cleaned up if window is closed
                unsafe {
                    if let Some(state) = &VPN_STATE {
                        state.running.store(false, Ordering::SeqCst);
                        cleanup_routes(&state.server_ip, &state.gateway);
                    }
                }
            }
            _ => {}
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
