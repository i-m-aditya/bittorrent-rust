use std::{
    cmp::min,
    collections::HashMap,
    future::Future,
    net::Ipv4Addr,
    pin::Pin,
    sync::{mpsc, Arc},
    task::{Context, Poll},
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{
        mpsc::{Receiver, Sender},
        Mutex,
    },
};

use crate::{cli::PeerRequest, hasher::bytes_to_hex, CHUNKSIZE};

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

pub struct PeerResponse {
    pub data: Vec<u8>,
    pub piece: u32,
}

pub struct PeerConnection {
    pub stream: TcpStream,
    pub peer_address: String,
    pub response_tx: Sender<PeerResponse>,
}

impl PeerConnection {
    pub async fn new(peer_address: String, response_tx: Sender<PeerResponse>) -> Self {
        // let address = format!("{}:{}", peer_address.0, peer_address.1);
        let stream = TcpStream::connect(peer_address.clone()).await.unwrap();
        PeerConnection {
            stream,
            peer_address,
            response_tx,
        }
    }

    // pub async fn temp(peer_address: String)
    pub async fn establish_connection(&mut self, infohash: Arc<[u8; 20]>) {
        self.handshake(infohash, None).await;
        self.wait(PeerMessage::Bitfield).await;
        self.send_interested().await;
        self.wait(PeerMessage::Unchoke).await;
    }

    pub async fn download_and_respond_piece(&mut self, piece_index: u32, piece_length: u32) {
        let mut i = 0;
        let mut piece_data_in_bytes = Vec::new();
        while i < piece_length {
            let block_length = min(CHUNKSIZE, piece_length - i);

            self.send_request(piece_index as u32, i as u32, block_length as u32)
                .await;

            let payload = self.wait(PeerMessage::Piece).await;
            // Verify we got the right piece and offset
            let received_index =
                u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
            let received_begin =
                u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);

            if received_index != piece_index as u32 || received_begin != i as u32 {
                panic!(
                    "Received wrong piece data: expected piece {}, offset {}, got piece {}, offset {}",
                    piece_index, i, received_index, received_begin
                );
            }

            piece_data_in_bytes.extend_from_slice(&payload[8..]);

            i += block_length;
        }

        let _ = self
            .response_tx
            .send(PeerResponse {
                data: piece_data_in_bytes,
                piece: piece_index,
            })
            .await;
    }
    pub async fn handshake(&mut self, infohash: Arc<[u8; 20]>, extension: Option<bool>) -> String {
        // Construct a message
        let mut message = vec![19];
        message.extend(b"BitTorrent protocol"); // 19 bytes
        message.extend([0u8; 8]);
        message.extend(*infohash);
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

pub struct PeerManager {
    peer_request_rx: Arc<Mutex<Receiver<PeerRequest>>>,
    peer_response_tx: Sender<PeerResponse>,
}

impl PeerManager {
    pub async fn new(request_rx: Receiver<PeerRequest>, response_tx: Sender<PeerResponse>) -> Self {
        PeerManager {
            peer_request_rx: Arc::new(Mutex::new(request_rx)),
            peer_response_tx: response_tx,
        }
    }

    pub async fn spawn_peers(&self, peer_addresses: Vec<String>, infohash: Arc<[u8; 20]>) {
        let peer_request_rx = self.peer_request_rx.clone();
        // let infohash = Arc::new(infohash);
        for peer_address in peer_addresses {
            let infohash = infohash.clone();

            let peer_request_rx = peer_request_rx.clone();
            let peer_response_tx = self.peer_response_tx.clone();
            let _handle = tokio::spawn(peer_worker(
                peer_address.clone(),
                infohash,
                peer_response_tx,
                peer_request_rx,
            ));
        }
    }
}

pub async fn peer_worker(
    peer_address: String,
    infohash: Arc<[u8; 20]>,
    response_tx: Sender<PeerResponse>,
    request_rx: Arc<Mutex<Receiver<PeerRequest>>>,
) {
    let mut connection = PeerConnection::new(peer_address, response_tx).await;
    connection.establish_connection(infohash).await;

    loop {
        let req = {
            let mut rx = request_rx.lock().await; // Lock briefly
            match rx.recv().await {
                Some(req) => req,
                None => break, // Channel closed, exit loop
            }
        }; // Lock released immediately

        match req {
            PeerRequest::DowloadPiece {
                piece_index,
                piece_length,
            } => {
                connection
                    .download_and_respond_piece(piece_index, piece_length)
                    .await;
            }
        }
    }
}
