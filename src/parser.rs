use std::{cmp::min, env::current_dir, fs::File, io::Read, net::Ipv4Addr};

use anyhow::{anyhow, Error, Ok, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::{hasher::bytes_to_hex_url_encoded, request::TrackerResponse};
#[derive(Debug, Default)]
pub struct Parser;

impl Parser {
    pub fn parse_torrent_file(input: &[u8]) -> Result<TorrentFile> {
        serde_bencode::from_bytes(input).map_err(|e| anyhow!("Failed to parse input: {}", e))
    }
}
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TorrentFile {
    pub announce: String,
    pub info: TorrentInfo,
}
#[derive(Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct TorrentInfo {
    pub length: u32,
    pub name: String,
    #[serde(rename = "piece length")]
    pub piece_length: u32,
    #[serde(with = "serde_bytes")]
    pub pieces: Vec<u8>,
}

impl TorrentFile {
    pub fn parse_file_from_path(path: &String) -> anyhow::Result<TorrentFile> {
        let file_path = current_dir().unwrap().join(path);
        let mut file = File::open(file_path).unwrap();

        let mut contents = Vec::new();
        file.read_to_end(&mut contents).unwrap();

        let torrent_file = Parser::parse_torrent_file(contents.as_ref()).unwrap();
        Ok(torrent_file)
    }

    pub async fn discover_peers(&self) -> Result<Vec<(Ipv4Addr, u16)>, Error> {
        let client = Client::new();

        let url_encoded_info_hash =
            bytes_to_hex_url_encoded(&serde_bencode::to_bytes(&self.info).unwrap());
        let url = format!("{}?info_hash={}", self.announce, url_encoded_info_hash);

        let req = client
            .get(url)
            .query(&[
                ("peer_id", String::from("-TR2940-5f2b3b3b3b3b")),
                ("port", String::from("6881")),
                ("uploaded", String::from("0")),
                ("downloaded", String::from("0")),
                ("left", self.info.length.to_string()),
                ("compact", String::from("1")),
            ])
            .build()?;
        let response = client.execute(req).await?;
        let response = response.bytes().await?;

        let tracker_response = serde_bencode::from_bytes::<TrackerResponse>(&response)?;

        let mut peers = Vec::<(Ipv4Addr, u16)>::new();
        for peer in tracker_response.peers.chunks(6) {
            let mut ip = [0u8; 4];
            ip.copy_from_slice(&peer[..4]);
            let port = u16::from_be_bytes([peer[4], peer[5]]);
            peers.push((Ipv4Addr::from(ip), port));
        }
        Ok(peers)
    }

    pub fn piece_and_length(&self) -> Vec<(u32, u32)> {
        let total_length = self.info.length;
        let mut piece_index_and_length = Vec::new();

        let mut piece_offset = 0;
        let mut piece_length;
        let mut piece_index = 0;
        let standard_piece_length = self.info.piece_length;
        while piece_offset < total_length {
            piece_length = min(total_length - piece_offset, standard_piece_length);

            piece_index_and_length.push((piece_index, piece_length));
            piece_offset += standard_piece_length;
            piece_index += 1;
        }

        piece_index_and_length
    }
}

mod tests {
    #[allow(unused_imports)]
    use super::*;

    #[test]
    fn test_torrent_file_ser_deser() {
        let torrent_file = TorrentFile::default();

        let serialized_tf = serde_bencode::to_bytes(&torrent_file).unwrap();
        let deserialized_tf = Parser::parse_torrent_file(&serialized_tf).unwrap();

        assert_eq!(torrent_file, deserialized_tf);
    }
}
