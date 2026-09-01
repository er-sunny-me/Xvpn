use clap::Parser;
use shared::Message;
use std::net::{SocketAddr, UdpSocket};
use std::sync::{Arc, Mutex};
use std::thread;
use std::io::{Read, Write};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    #[arg(long, default_value_t = 51820)]
    port: u16,

    #[arg(long, default_value = "10.8.0.1")]
    tun_ip: String,

    #[arg(long, default_value = "255.255.255.0")]
    tun_netmask: String,

    #[arg(long, default_value = "tun0")]
    tun_name: String,
}

fn main() {
    let args = Args::parse();
    let addr = format!("{}:{}", args.host, args.port);
    let socket = UdpSocket::bind(&addr).expect("Failed to bind socket");
    println!("Server listening on {}", addr);

    let mut config = tun::Configuration::default();
    config
        .address(args.tun_ip.parse::<std::net::Ipv4Addr>().unwrap())
        .netmask(args.tun_netmask.parse::<std::net::Ipv4Addr>().unwrap())
        .tun_name(args.tun_name)
        .up();

    let tun_device = tun::create(&config).expect("Failed to create TUN device. Are you running as root?");
    println!("TUN device created successfully.");
    let (mut tun_reader, mut tun_writer) = tun_device.split();

    let socket = Arc::new(socket);
    let socket_rx = Arc::clone(&socket);
    let socket_tx = Arc::clone(&socket);

    // Track the client's last known address so we can forward TUN packets to it
    let last_client_addr: Arc<Mutex<Option<SocketAddr>>> = Arc::new(Mutex::new(None));
    let last_client_addr_rx = Arc::clone(&last_client_addr);
    let last_client_addr_tx = Arc::clone(&last_client_addr);

    // Thread 1: Read from UDP, Write to TUN
    thread::spawn(move || {
        let mut buf = [0u8; 65535];
        loop {
            match socket_rx.recv_from(&mut buf) {
                Ok((size, src)) => {
                    // Update client addr
                    if let Ok(mut addr_lock) = last_client_addr_rx.lock() {
                        *addr_lock = Some(src);
                    }

                    if let Ok(msg) = bincode::deserialize::<Message>(&buf[..size]) {
                        match msg {
                            Message::Ping { seq, payload } => {
                                let pong = Message::Pong { seq, payload };
                                let encoded = bincode::serialize(&pong).unwrap();
                                let _ = socket_rx.send_to(&encoded, src);
                            }
                            Message::IpPacket { payload } => {
                                let _ = tun_writer.write_all(&payload);
                            }
                            _ => {}
                        }
                    }
                }
                Err(e) => {
                    eprintln!("Error receiving UDP data: {}", e);
                }
            }
        }
    });

    // Thread 2 (Main thread): Read from TUN, Write to UDP
    let mut tun_buf = [0u8; 65535];
    loop {
        let read_res = tun_reader.read(&mut tun_buf);
        
        match read_res {
            Ok(size) if size > 0 => {
                let msg = Message::IpPacket { payload: tun_buf[..size].to_vec() };
                if let Ok(encoded) = bincode::serialize(&msg) {
                    let dest = {
                        let addr_lock = last_client_addr_tx.lock().unwrap();
                        *addr_lock
                    };
                    if let Some(client_addr) = dest {
                        let _ = socket_tx.send_to(&encoded, client_addr);
                    }
                }
            }
            Ok(_) => {}, // 0 bytes read
            Err(e) => {
                // If it's a WouldBlock error, just sleep briefly. But by default it's blocking.
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!("Error reading from TUN: {}", e);
                }
            }
        }
    }
}
