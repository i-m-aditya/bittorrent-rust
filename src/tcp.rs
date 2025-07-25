use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
};

use crate::hasher::bytes_to_hex;

pub struct Connection {
    pub stream: TcpStream,
}
//

#[derive(Clone, Copy, Debug)]
pub enum PeerMessage {
    Choke,
    Unchoke,
    Interested,
    NotInterested,
    Have,
    Bitfield,
    Request,
    Piece,
    Cancel,
}
impl Connection {
    pub async fn new(address: &String) -> Self {
        let stream = TcpStream::connect(address).await.unwrap();
        Connection { stream }
    }

    pub async fn handshake(&mut self, infohash: &Vec<u8>) -> String {
        // Construct a message
        let mut message = vec![19];
        message.extend(b"BitTorrent protocol"); // 19 bytes
        message.extend([0u8; 8]);
        message.extend(infohash);
        message.extend(b"00112233445566778899");

        self.stream.write(&message).await.unwrap();

        let mut response = vec![0; message.len()];
        self.stream.read(&mut response).await.unwrap();

        let response_peer_id = &response[response.len() - 20..];
        // println!("Peer ID: {}", bytes_to_hex(response_peer_id));
        return bytes_to_hex(response_peer_id);
    }

    pub async fn wait(&mut self, id: PeerMessage) -> Vec<u8> {
        // println!("Peer Message: {:?}", id);
        let mut length_buf = [0; 4];
        self.stream.read_exact(&mut length_buf).await.unwrap();

        let mut msg_type = [0; 1];
        self.stream
            .read_exact(&mut msg_type)
            .await
            .expect("Failed to read mssage id");
        if msg_type[0] != id.clone() as u8 {
            panic!("Expected msg id {}, got {}", id as u8, msg_type[0]);
        }

        let payload_size = u32::from_be_bytes(length_buf) - 1;
        let mut payload = vec![0; payload_size as usize];
        self.stream
            .read_exact(&mut payload)
            .await
            .expect("Failed to read payload");
        return payload;
    }

    pub async fn send_interested(&mut self) {
        self.send_message(PeerMessage::Interested, vec![]).await;
    }

    pub async fn send_request(&mut self, piece_index: u32, begin: u32, length: u32) {
        let mut payload = vec![0; 12];

        payload[0..4].copy_from_slice(&piece_index.to_be_bytes());
        payload[4..8].copy_from_slice(&begin.to_be_bytes());
        payload[8..12].copy_from_slice(&length.to_be_bytes());
        self.send_message(PeerMessage::Request, payload).await;
    }

    pub async fn send_message(&mut self, id: PeerMessage, payload: Vec<u8>) {
        let mut msg = vec![0; 5 + payload.len()];
        let mut length = payload.len() as u32;
        if length == 0 {
            length = 1;
        }
        // println!("Hello");
        msg[0..4].copy_from_slice(&(length).to_be_bytes());
        msg[4] = id as u8;
        msg[5..].copy_from_slice(&payload);

        self.stream.write_all(&msg).await.unwrap();
    }
}
