use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub enum Message {
    Ping { seq: u64, payload: Vec<u8> },
    Pong { seq: u64, payload: Vec<u8> },
    IpPacket { payload: Vec<u8> },
}
