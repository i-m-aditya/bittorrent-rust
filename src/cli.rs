use std::{
    cmp::min,
    env,
    net::{Ipv4Addr, SocketAddrV4},
    path::Path,
    sync::Arc,
};

use clap::{Parser, Subcommand};
use tokio::{fs, sync::Semaphore, task::JoinSet};

use crate::{
    hasher::{bytes_to_hex, hash_bytes, hash_bytes_and_hex},
    parser::TorrentFile,
    tcp::{Connection, PeerMessage},
    util::decode_bencoded_value,
    CHUNKSIZE,
};

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
        #[arg(short, long)]
        address: Option<SocketAddrV4>,

        #[arg(short, long, help = "File path")]
        output: Option<String>,

        #[arg(short, long, help = "Debug mode")]
        debug: Option<bool>,

        file_path: String,

        piece: usize,
    },

    Download {
        #[arg(short)]
        output: String,

        file_path: String,
    },

    #[command(name = "magnet_parse")]
    MagnetParse {
        magnet_link: String,
    },
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

                let mut connection = Connection::new(&url).await;
                let peer_id = connection.handshake(&infohash.to_vec()).await;

                println!("Peer ID: {}", peer_id);
            }
            Commands::DownloadPiece {
                output,
                file_path,
                piece,
                ..
            } => {
                // println!("Download Piece Args: {:?}", download_piece_args);

                let torrent_file = TorrentFile::parse_file_from_path(&file_path)?;

                let peers = torrent_file.discover_peers().await?;

                let remote_peer = format!("{}:{}", peers[0].0, peers[0].1);

                // println!("Peer1 : {}", peer1);
                let infohash = hash_bytes(&serde_bencode::to_bytes(&torrent_file.info)?);
                let mut connection = Connection::new(&remote_peer).await;

                let _ = connection.handshake(&infohash.to_vec()).await;

                // println!("Peer Id: {}", peer_id);

                connection.wait(PeerMessage::Bitfield).await;
                connection.send_interested().await;
                connection.wait(PeerMessage::Unchoke).await;

                let piece_index = piece;
                let piece_length = torrent_file.info.piece_length;

                println!(
                    "Piece index: {} ; Piece length: {}",
                    piece_index, piece_length
                );

                let piece_index = piece;
                let total_length = torrent_file.info.length;
                let standard_piece_length = torrent_file.info.piece_length;

                // Calculate the actual length of this specific piece
                let piece_length = {
                    let start_byte = piece_index as u32 * standard_piece_length;
                    let end_byte = std::cmp::min(start_byte + standard_piece_length, total_length);
                    end_byte - start_byte
                };

                let piece_data = download_piece_async(
                    peers[0],
                    Arc::new(infohash),
                    piece_index as u32,
                    piece_length,
                )
                .await;

                fs::write(
                    env::current_dir()?.join(output.clone().unwrap()),
                    piece_data,
                )
                .await?;
                println!("Piece {} downloaded to {}.", piece_index, output.unwrap());
            }

            Commands::Download { output, file_path } => {
                let torrent_file = TorrentFile::parse_file_from_path(&file_path)?;

                let peers = torrent_file.discover_peers().await?;
                let piece_index_and_length = torrent_file.piece_and_length();

                let semaphore = Arc::new(Semaphore::new(5));

                let infohash = Arc::new(hash_bytes(&serde_bencode::to_bytes(&torrent_file.info)?));

                let mut tasks = JoinSet::new();
                for (piece_index, piece_length) in piece_index_and_length {
                    let semaphore = semaphore.clone();
                    let peer = peers[piece_index as usize % peers.len()].clone(); // Round-robin peers
                    let infohash = infohash.clone();
                    tasks.spawn(async move {
                        let _permit = semaphore.acquire();
                        download_piece_async(peer, infohash, piece_index, piece_length).await
                    });
                }

                let mut pieces = Vec::new();
                while let Some(Ok(data)) = tasks.join_next().await {
                    // println!("Downloaded piece: {:?}", data);
                    pieces.push(data);
                }

                fs::write(env::current_dir()?.join(output), pieces.concat()).await?;
            }
            Commands::MagnetParse { magnet_link } => {
                // magnet:?xt=urn:btih:ad42ce8109f54c99613ce38f9b4d87e70f24a165&dn=magnet1.gif&tr=http%3A%2F%2Fbittorrent-test-tracker.codecrafters.io%2Fannounce
                //magnet:?xt={info_hash}&dn={file_name}&tr={tracker_url}

                let split_values = magnet_link
                    .split('?')
                    .nth(1)
                    .unwrap()
                    .split('&')
                    .collect::<Vec<_>>();

                let info_hash = split_values[0]
                    .split("=")
                    .nth(1)
                    .unwrap()
                    .split(":")
                    .nth(2)
                    .unwrap();
                let _file_name = split_values[1].split("=").nth(1).unwrap();
                let tracker = split_values[2].split("=").nth(1).unwrap();

                let tracker_decoded = urlencoding::decode(tracker).unwrap();

                // let (value, _) = decode_bencoded_value(tracker, 0);
                println!("Tracker URL: {}", tracker_decoded);
                println!("Info Hash: {}", info_hash);
            }
        }

        Ok(())
    }

    pub fn test(&self) {
        unimplemented!("Do nonthingß");
    }
}

async fn download_piece_async(
    peer: (Ipv4Addr, u16),
    infohash: Arc<[u8; 20]>,
    piece_index: u32,
    piece_length: u32,
) -> Vec<u8> {
    let remote_peer = format!("{}:{}", peer.0, peer.1);
    let mut connection = Connection::new(&remote_peer).await;
    connection.handshake(&infohash.to_vec()).await;
    connection.wait(PeerMessage::Bitfield).await;
    connection.send_interested().await;
    connection.wait(PeerMessage::Unchoke).await;

    let mut i = 0;
    let mut piece_data_in_bytes = Vec::new();
    while i < piece_length {
        let block_length = min(CHUNKSIZE, piece_length - i);

        connection
            .send_request(piece_index as u32, i as u32, block_length as u32)
            .await;

        let payload = connection.wait(PeerMessage::Piece).await;
        // Verify we got the right piece and offset
        let received_index = u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let received_begin = u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);

        if received_index != piece_index as u32 || received_begin != i as u32 {
            panic!(
                "Received wrong piece data: expected piece {}, offset {}, got piece {}, offset {}",
                piece_index, i, received_index, received_begin
            );
        }

        piece_data_in_bytes.extend_from_slice(&payload[8..]);

        i += block_length;
    }

    piece_data_in_bytes
}
