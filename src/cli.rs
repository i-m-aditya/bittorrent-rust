use std::{
    cmp::min,
    collections::HashMap,
    env,
    fmt::format,
    io::Read,
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    sync::{Arc, Mutex},
};

use clap::{Parser, Subcommand};
use reqwest::Client;
use tokio::{
    fs,
    sync::{mpsc, Semaphore},
    task::JoinSet,
};

use crate::{
    hasher::{bytes_to_hex, hash_bytes, hash_bytes_and_hex},
    parser::TorrentFile,
    request::TrackerResponse,
    tcp::{PeerConnection, PeerManager, PeerMessage},
    util::{decode_bencoded_value, decode_magnet_link},
    CHUNKSIZE,
};

#[derive(Debug)]
struct Peer {
    peer_id: (Ipv4Addr, u16),
}

impl Peer {
    pub fn get_formatted_peer_id(&self) -> String {
        format!("{}:{}", self.peer_id.0, self.peer_id.1)
    }
}

#[derive(Debug)]
pub enum PeerRequest {
    DowloadPiece { piece_index: u32, piece_length: u32 },
}
#[derive(Parser, Debug)]
pub struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
enum Commands {
    Decode {
        value: String,
    },

    Info {
        path: String,
    },

    Peers {
        path: String,
    },

    Handshake {
        path: String,
        url: String,
    },

    #[command(name = "download_piece")]
    DownloadPiece {
        #[command(flatten)]
        metadata: DownloadMetadata,
    },

    Download {
        #[command(flatten)]
        metadata: DownloadMetadata,
    },

    #[command(name = "magnet_parse")]
    MagnetParse {
        magnet_link: String,
    },

    #[command(name = "magnet_handshake")]
    MagnetHandshake {
        magnet_link: String,
    },
}

#[derive(Debug, Parser, Clone)]
struct DownloadMetadata {
    #[arg(short)]
    output: String,
    file_path: String,
    piece: Option<u32>,
}

impl Cli {
    pub async fn execute(self) -> anyhow::Result<()> {
        match self.command {
            Commands::Decode { value: string } => {
                let (decoded_value, _) = decode_bencoded_value(&string, 0);
                println!("{}", decoded_value.to_string());
            }
            Commands::Info { path } => {
                let torrent_file = TorrentFile::parse_file_from_path(&path)?;

                println!("Tracker URL: {}", torrent_file.announce);
                println!("Length: {}", torrent_file.info.length);

                println!(
                    "Info Hash: {}",
                    hash_bytes_and_hex(&serde_bencode::to_bytes(&torrent_file.info)?)
                );

                println!("Piece Length: {}", torrent_file.info.piece_length);

                println!("Piece Hashes:");

                for piece in torrent_file.info.pieces.chunks(20) {
                    println!("{}", bytes_to_hex(piece));
                }
            }
            Commands::Peers { path } => {
                let torrent_file = TorrentFile::parse_file_from_path(&path)?;
                println!("Torrent File: {:?}", torrent_file);

                let peers = torrent_file.discover_peers().await?;

                for peer in peers {
                    println!("{}:{}", peer.0, peer.1);
                }

                // let mut peers: Vec<String> = Vec::new();
            }
            Commands::Handshake { path, url } => {
                let torrent_file = TorrentFile::parse_file_from_path(&path)?;
                let infohash = hash_bytes(&serde_bencode::to_bytes(&torrent_file.info)?);

                let (temp_tx, _) = tokio::sync::mpsc::channel(1000);
                let mut connection = PeerConnection::new(url, temp_tx).await;
                let peer_id = connection.handshake(Arc::new(infohash), None).await;

                println!("Peer ID: {}", peer_id);
            }

            Commands::Download { metadata } | Commands::DownloadPiece { metadata } => {
                let DownloadMetadata {
                    output,
                    file_path,
                    piece,
                } = metadata;

                let torrent_file = TorrentFile::parse_file_from_path(&file_path)?;

                let peers = torrent_file.discover_peers().await?;

                let peers = if piece.is_some() {
                    vec![peers[0]]
                } else {
                    peers
                };

                let piece_index_and_length = if piece.is_some() {
                    let piece_length = {
                        let start_byte = piece.unwrap() as u32 * torrent_file.info.piece_length;
                        let end_byte = std::cmp::min(
                            start_byte + torrent_file.info.piece_length,
                            torrent_file.info.length,
                        );
                        end_byte - start_byte
                    };
                    vec![(piece.unwrap(), piece_length)]
                } else {
                    torrent_file.piece_and_length()
                };

                let infohash = Arc::new(hash_bytes(&serde_bencode::to_bytes(&torrent_file.info)?));

                let (peer_request_tx, peer_request_rx) = tokio::sync::mpsc::channel(1000);
                let (peer_response_tx, mut peer_response_rx) = tokio::sync::mpsc::channel(1000);

                let peer_manager = PeerManager::new(peer_request_rx, peer_response_tx).await;

                let peer_addresses = peers
                    .iter()
                    .map(|(ip, val)| format!("{}:{}", ip, val))
                    .collect::<Vec<_>>();
                peer_manager
                    .spawn_peers(peer_addresses, infohash.clone())
                    .await;

                let total_pieces = piece_index_and_length.len();

                for (piece_index, piece_length) in piece_index_and_length {
                    let peer_request = PeerRequest::DowloadPiece {
                        piece_index,
                        piece_length,
                    };
                    peer_request_tx.send(peer_request).await.unwrap();
                }

                // Close the request channel so peer workers know when to exit
                drop(peer_request_tx);

                // Collect responses - we know exactly how many to expect
                let mut pieces = Vec::new();
                for _ in 0..total_pieces {
                    if let Some(data) = peer_response_rx.recv().await {
                        println!("Received piece: {:?}", data.piece);
                        pieces.push(data);
                    }
                }

                // Sort pieces by index to maintain correct order
                pieces.sort_by(|a, b| a.piece.cmp(&b.piece));

                let mut contents = Vec::new();
                for piece in pieces {
                    contents.extend(piece.data);
                }

                fs::write(env::current_dir()?.join(output), contents).await?;
            }
            Commands::MagnetParse { magnet_link } => {
                // magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce
                //magnet:?xt={info_hash}&dn={file_name}&tr={tracker_url}

                let (tracker_decoded, info_hash, _) = decode_magnet_link(&magnet_link);
                // let (value, _) = decode_bencoded_value(tracker, 0);
                println!("Tracker URL: {}", tracker_decoded);
                println!("Info Hash: {}", info_hash);
            }
            Commands::MagnetHandshake { magnet_link } => {
                // magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce
                //magnet:?xt={info_hash}&dn={file_name}&tr={tracker_url}
                let (tracker_url, info_hash, file_name) = decode_magnet_link(&magnet_link);

                let info_hash: [u8; 20] = info_hash
                    .as_bytes()
                    .try_into()
                    .expect("Info hash must be 20 bytes");

                let torrent_file = TorrentFile::parse_file_from_path(&file_name).unwrap();
                println!("annoce: {}", torrent_file.announce);

                let client = Client::new();

                let req = client.get(&tracker_url).build().unwrap();
                let response = client.execute(req).await?.bytes().await?;

                let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&response)?;

                let mut peers = Vec::<(Ipv4Addr, u16)>::new();
                for peer in tracker_response.peers.chunks(6) {
                    let mut ip = [0u8; 4];
                    ip.copy_from_slice(&peer[..4]);
                    let port = u16::from_be_bytes([peer[4], peer[5]]);
                    peers.push((Ipv4Addr::from(ip), port));
                }
                let peer_address = format!("{}:{}", peers[0].0, peers[0].1);
                let (temp_tx, _) = tokio::sync::mpsc::channel(1000);
                let mut connection = PeerConnection::new(peer_address, temp_tx).await;
                let peer_id = connection.handshake(Arc::new(info_hash), Some(true)).await;

                println!("Peer ID: {}", peer_id);
            }
        }

        Ok(())
    }

    pub fn test(&self) {
        unimplemented!("Do nonthingß");
    }
}
