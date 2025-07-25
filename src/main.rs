#![allow(dead_code)]
#![allow(unused_imports)]
use anyhow::Error;
use clap::{Parser, Subcommand};

use std::{cmp::min, env, fs, net::SocketAddrV4, path::Path, str::FromStr};

use tcp::{Connection, PeerMessage};
mod tcp;

use crate::{hasher::hash_bytes_and_hex, parser::TorrentFile};

mod hasher;
mod parser;
mod request;
// Available if you need it!
use serde_bencode;
//dgddggs

use hasher::{bytes_to_hex, hash_bytes};

const CHUNKSIZE: u32 = 16 * 1024;

fn find_e_for_index(s: &str, index: usize) -> usize {
    let mut count = 1;

    let chars = s.char_indices().skip(index + 1);

    for (i, ch) in chars {
        if ch == 'e' {
            count -= 1;
        } else if ['l', 'd', 'i'].contains(&ch) {
            count += 1;
        }

        if count == 0 {
            return i;
        }
    }

    return 0;
}

#[derive(Parser, Debug)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Decode {
        string: String,
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
}

fn decode_bencoded_value(encoded_value: &str, index: usize) -> (serde_json::Value, usize) {
    // println!("encoded_value: {}", encoded_value);
    if encoded_value.chars().nth(index).unwrap().is_digit(10) {
        let parts: Vec<&str> = encoded_value[index..].split(":").collect();
        let num_string = parts[0].to_string();
        let num_integer = num_string.parse::<i32>().unwrap();

        let start = index + num_string.len() + 1;
        let end = start + num_integer as usize;

        // println!("start {}, end {}, len {}", start, end, encoded_value.len());
        if end > encoded_value.len() {
            return (
                serde_json::Value::String("".to_string()),
                encoded_value.len(),
            );
        }

        let decoded_string = &encoded_value[start..end];

        // println!("decoded string {}, end {}", decoded_string, end);
        return (serde_json::Value::String(decoded_string.to_string()), end);
    } else if encoded_value.chars().nth(index).unwrap() == 'i' {
        let e_position = find_e_for_index(encoded_value, index);

        let parsed_value = &encoded_value[index + 1..e_position];

        // println!("decoded string {}, end {}", parsed_value, e_position + 1);

        return (
            serde_json::Value::Number(parsed_value.parse::<i64>().unwrap().into()),
            e_position + 1,
        );
    } else if encoded_value.chars().nth(index).unwrap() == 'l' {
        let mut i = index + 1;

        // println!("i : {}", i);

        let mut lst: Vec<serde_json::Value> = Vec::new();

        while i < encoded_value.len() {
            if encoded_value.chars().nth(i).unwrap() == 'e' {
                break;
            } else {
                let (decoded_value, new_index) = decode_bencoded_value(encoded_value, i);
                // println!(
                //     "decoded_value {}, new_index {}",
                //     decoded_value.to_string(),
                //     new_index
                // );
                lst.push(decoded_value);
                i = new_index;
            }
        }

        // println!("decoded list {:?}, end {}", lst, i + 1);
        return (serde_json::Value::Array(lst), i + 1);
    } else if encoded_value.chars().nth(index).unwrap() == 'd' {
        // println!(" hello dict, index: {}, len {}", index, encoded_value.len());
        let mut i = index + 1;

        let mut dict: serde_json::Map<String, serde_json::Value> = serde_json::Map::new();

        while i < encoded_value.len() {
            if encoded_value.chars().nth(i).unwrap() == 'e' {
                break;
            } else {
                let (decoded_key, new_index) = decode_bencoded_value(encoded_value, i);
                let (decoded_value, new_index) = decode_bencoded_value(encoded_value, new_index);
                dict.insert(decoded_key.as_str().unwrap().to_string(), decoded_value);
                i = new_index;
            }
        }
        // println!("End dict {:?}, end {}", dict, i + 1);
        return (serde_json::Value::Object(dict), i + 1);
    } else {
        panic!("Not implemented")
    }
}

// Usage: your_bittorrent.sh decode "<encoded_value>"

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Decode { string } => {
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

            let mut connection = Connection::new(&url);
            let peer_id = connection.handshake(&infohash.to_vec());

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
            let mut connection = Connection::new(&remote_peer);

            let _ = connection.handshake(&infohash.to_vec());

            // println!("Peer Id: {}", peer_id);

            connection.wait(PeerMessage::Bitfield);
            connection.send_interested();
            connection.wait(PeerMessage::Unchoke);

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
            let mut i = 0;
            let mut piece = Vec::new();
            while i < piece_length {
                let block_length = min(CHUNKSIZE, piece_length - i);
                println!(
                    "Requesting block {} of block_length {}, piece_length {}",
                    i, block_length, piece_length
                );
                connection.send_request(piece_index as u32, i as u32, block_length as u32);

                let payload = connection.wait(PeerMessage::Piece);
                // Verify we got the right piece and offset
                let received_index =
                    u32::from_be_bytes([payload[0], payload[1], payload[2], payload[3]]);
                let received_begin =
                    u32::from_be_bytes([payload[4], payload[5], payload[6], payload[7]]);

                if received_index != piece_index as u32 || received_begin != i as u32 {
                    panic!("Received wrong piece data: expected piece {}, offset {}, got piece {}, offset {}", 
               piece_index, i, received_index, received_begin);
                }

                piece.extend_from_slice(&payload[8..]);

                i += block_length;
            }

            let output_path = output.unwrap_or_else(|| panic!("No output path provided!"));
            std::fs::write(env::current_dir()?.join(Path::new(&output_path)), piece)?;
            println!("Piece {} downloaded to {}.", piece_index, &output_path)
        }

        Commands::Download { output, file_path } => {
            let torrent_file = TorrentFile::parse_file_from_path(&file_path)?;

            let peers = torrent_file.discover_peers().await?;

            let remote_peer = format!("{}:{}", peers[0].0, peers[0].1);

            // println!("Peer1 : {}", peer1);
            let infohash = hash_bytes(&serde_bencode::to_bytes(&torrent_file.info)?);
            let mut connection = Connection::new(&remote_peer);

            let _ = connection.handshake(&infohash.to_vec());

            // println!("Peer Id: {}", peer_id);

            connection.wait(PeerMessage::Bitfield);
            connection.send_interested();
            connection.wait(PeerMessage::Unchoke);

            let total_length = torrent_file.info.length;
            let standard_piece_length = torrent_file.info.piece_length;
            let mut file_data = Vec::new();

            let mut piece_offset = 0;
            let mut piece_index = 0;
            while piece_offset < total_length {
                let piece_length = min(total_length - piece_offset, standard_piece_length);
                println!(
                    "Piece_index: {}, piece_offset: {}, piece_length:{}, total_length: {}",
                    piece_index, piece_offset, piece_length, total_length
                );
                let piece_data = download_piece(&mut connection, piece_index, piece_length);
                file_data.extend(piece_data);

                piece_offset += standard_piece_length;
                piece_index += 1;
            }
            fs::write(output, file_data).expect("Failed to write file data");
        }
    }

    Ok(())
}

fn download_piece(connection: &mut Connection, piece_index: u32, piece_length: u32) -> Vec<u8> {
    let mut i = 0;
    let mut piece_data_in_bytes = Vec::new();
    while i < piece_length {
        let block_length = min(CHUNKSIZE, piece_length - i);

        connection.send_request(piece_index as u32, i as u32, block_length as u32);

        let payload = connection.wait(PeerMessage::Piece);
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
