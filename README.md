# BitTorrent Client – Rust Implementation

## Overview

A fully functional BitTorrent client built from scratch in Rust that implements the core BitTorrent protocol. This project demonstrates a complete understanding of peer-to-peer file sharing, including torrent file parsing, peer discovery, the BitTorrent wire protocol, and efficient piece-based downloading with integrity verification.

## ✨ Key Features

- **🔧 Complete Bencoding Support**: Encode and decode all bencoded data types (strings, integers, lists, dictionaries) per BEP-3 specification
- **📄 Torrent File Parsing**: Extract and process complete torrent metadata including announce URLs, file structure, and piece information  
- **🔐 Info Hash Calculation**: Generate SHA-1 hashes for torrent identification and swarm participation
- **🌐 Peer Discovery**: Connect to HTTP/HTTPS trackers and parse compact peer lists
- **🤝 BitTorrent Wire Protocol**: Full implementation of peer messaging including handshake, bitfield, interested/uninterested, choke/unchoke, request, and piece messages
- **⚡ Concurrent Downloads**: Parallel piece downloading with configurable connection limits and round-robin peer selection
- **🧩 Piece Management**: Intelligent blockwise downloading (16KiB blocks) with request pipelining for optimal performance
- **✅ Integrity Verification**: SHA-1 hash validation for every downloaded piece ensuring data authenticity
- **💾 File Assembly**: Seamless reconstruction and disk writing of complete files from downloaded pieces
- **🎯 Robust Architecture**: Clean, modular design built for performance and extensibility

## 🚀 Quick Start

### Prerequisites
- Rust 1.87+ with Cargo
- Active internet connection for tracker communication

### Installation
```bash
git clone <repository-url>
cd bittorrent-rust
cargo build --release
```

### Usage

#### Decode Bencoded Data
```bash
./your_program.sh decode "d8:announce9:localhost4:spam4:eggse"
```

#### Extract Torrent Information
```bash
./your_program.sh info sample.torrent
```

#### Discover Peers
```bash
./your_program.sh peers sample.torrent
```

#### Handshake with Peer
```bash
./your_program.sh handshake sample.torrent <peer_ip:port>
```

#### Download Specific Piece
```bash
./your_program.sh download_piece -o /tmp/piece0 sample.torrent 0
```

#### Download Complete File
```bash
./your_program.sh download -o /tmp/downloaded_file sample.torrent
```

## 🏗️ Architecture

### Core Components

#### Bencoding Engine (`src/util.rs`)
- **Purpose**: Serialization format used throughout BitTorrent protocol
- **Functions**: `decode_bencoded_value()` with recursive parsing for nested structures
- **Supports**: Strings, integers, lists, dictionaries with proper error handling

#### Torrent Parser (`src/parser.rs`)
- **Purpose**: Parse .torrent files and extract metadata
- **Key Structs**: `TorrentFile`, `TorrentInfo`
- **Features**: 
  - Announce URL extraction
  - File length and piece information
  - Piece hash validation data
  - Async peer discovery via tracker communication

#### Peer Protocol (`src/tcp.rs`)
- **Purpose**: Implement BitTorrent wire protocol for peer communication
- **Key Features**:
  - Asynchronous TCP connection management
  - Protocol message handling (handshake, bitfield, interested, unchoke, request, piece)
  - Message parsing and state management

#### CLI Interface (`src/cli.rs`)
- **Purpose**: User-friendly command-line interface
- **Commands**: decode, info, peers, handshake, download_piece, download
- **Features**: Parallel downloading with semaphore-based concurrency control

#### Cryptographic Hashing (`src/hasher.rs`)
- **Purpose**: SHA-1 hash calculation for info hashes and piece verification
- **Functions**: `hash_bytes()`, `bytes_to_hex()`, URL encoding utilities

## 🔬 Protocol Implementation Details

### Piece Download Strategy
1. **Connection Setup**: Establish TCP connection and perform BitTorrent handshake
2. **State Exchange**: Wait for bitfield, send interested, wait for unchoke
3. **Block Requests**: Download pieces in 16KiB blocks with request pipelining
4. **Verification**: Validate each piece against stored SHA-1 hash
5. **Assembly**: Reconstruct complete file from verified pieces

### Concurrency Model
- **Parallel Downloads**: Up to 5 concurrent peer connections (configurable)
- **Round-Robin Peer Selection**: Distribute load across available peers
- **Async/Await**: Built on Tokio for efficient I/O operations

### Error Handling
- Comprehensive error handling with `anyhow` for user-friendly messages
- Graceful connection failures with peer fallback
- Data integrity checks prevent corrupted downloads

## 🛠️ Technical Stack

- **Language**: Rust 2021 Edition
- **Async Runtime**: Tokio for concurrent operations
- **HTTP Client**: Reqwest for tracker communication  
- **CLI Framework**: Clap with derive macros
- **Serialization**: Serde with Bencoding support
- **Cryptography**: SHA-1 hashing
- **Error Handling**: Anyhow for ergonomic error management

## 🔮 Planned Features

- **🧲 Magnet Link Support**: Parse and download files using magnet URIs without .torrent files (coming soon)
- **📊 Advanced Peer Selection**: Implement more sophisticated peer choosing algorithms
- **🏃 Performance Optimizations**: Additional request pipelining and caching strategies
- **🔧 Extended Protocol Support**: DHT, peer exchange, and other BEP implementations

## 📚 BitTorrent Protocol Concepts

- **Bencoding**: Compact binary serialization format for metadata exchange
- **Info Hash**: SHA-1 identifier calculated from torrent's info dictionary
- **Tracker**: Centralized service providing peer lists for content discovery
- **Piece**: File chunks (typically 256KB-1MB) that can be downloaded independently
- **Block**: Smaller chunks within pieces (16KiB) for network transfer efficiency
- **Wire Protocol**: Standardized message format for peer-to-peer communication

## 🤝 Contributing

This BitTorrent client was built to demonstrate a deep understanding of peer-to-peer networking and protocol implementation. Feel free to explore the code, suggest improvements, or use it as a learning resource for understanding BitTorrent protocol implementation.

Contributions are welcome! Please feel free to submit issues, feature requests, or pull requests.

## 📄 License

This project is open source. Feel free to use, modify, and distribute according to your needs.

---

**Happy torrenting! 🚀** Explore the fascinating world of peer-to-peer file sharing with this comprehensive BitTorrent implementation.
