#![allow(dead_code)]
#![allow(unused_imports)]
use anyhow::Error;
use clap::{Parser, Subcommand};

use std::{cmp::min, env, fs, net::SocketAddrV4, path::Path, str::FromStr};

use tcp::{PeerConnection, PeerMessage};
mod tcp;
mod util;

use crate::{cli::Cli, hasher::hash_bytes_and_hex, parser::TorrentFile};

mod cli;
mod hasher;
mod parser;
mod request;
// Available if you need it!
use serde_bencode;
//dgddggs

use hasher::{bytes_to_hex, hash_bytes};

const CHUNKSIZE: u32 = 16 * 1024;

// Usage: your_bittorrent.sh decode "<encoded_value>"

#[tokio::main]
async fn main() -> Result<(), Error> {
    let cli = Cli::parse();

    cli.execute().await?;

    Ok(())
}
