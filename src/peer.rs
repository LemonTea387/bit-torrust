use std::{
    error::Error,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpStream},
};

use byteorder::{BigEndian, ByteOrder, ReadBytesExt};

pub(crate) const PEER_ID: &str = "1337cafebabedeadbeef";

#[derive(Debug)]
pub struct Peer {
    ip_addr: IpAddr,
    port: u16,
}

impl TryFrom<&[u8]> for Peer {
    type Error = PeerError;
    fn try_from(value: &[u8]) -> Result<Self, Self::Error> {
        if value.len() != 6 {
            return Err(PeerError::UnknownBytesListFormat);
        }
        Ok(Self {
            ip_addr: IpAddr::V4(Ipv4Addr::new(value[0], value[1], value[2], value[3])),
            port: ((value[4] as u16) << 8) | value[5] as u16,
        })
    }
}

impl Peer {
    pub fn connect(&self, info_hash_slice: &[u8]) -> Result<PeerConnection, Box<dyn Error>> {
        let connection = TcpStream::connect((self.ip_addr, self.port))?;
        let mut info_hash = [0u8; 20];
        info_hash.clone_from_slice(info_hash_slice);
        Ok(PeerConnection {
            connection,
            info_hash,
        })
    }
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip_addr, self.port)
    }
}

pub struct PeerConnection {
    connection: TcpStream,
    info_hash: [u8; 20],
}

impl PeerConnection {
    pub fn handshake(mut self) -> Result<PeerConnectionReady, Box<dyn Error>> {
        // Try handshake
        // <19 in byte>BitTorrent protocol<8Bytes0><20byte sha1 info table hash><20peerid>
        let mut buf = [0u8; 68];
        buf[0] = 19;
        buf[1..20].clone_from_slice(b"BitTorrent protocol");
        buf[28..48].clone_from_slice(&self.info_hash);
        buf[48..].clone_from_slice(PEER_ID.as_bytes());

        match self.connection.write(&buf) {
            Ok(68) => {}
            _ => return Err(Box::new(PeerError::PeerHandshakeFailed)),
        }

        let mut response_buf = [0u8; 68];
        self.connection.read_exact(&mut response_buf)?;

        let mut peer_id = [0u8; 20];
        peer_id.clone_from_slice(&response_buf[48..]);

        Ok(PeerConnectionReady {
            connection: self.connection,
            peer_id,
        })
    }
}

pub struct PeerConnectionReady {
    connection: TcpStream,
    peer_id: [u8; 20],
}


#[repr(u8)]
enum PeerMessage {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield(Vec<u8>) = 5,
    Request {
        index: usize,
        begin: usize,
        length: usize,
    } = 6,
    Piece {
        index: usize,
        begin: usize,
        piece: Vec<u8>,
    } = 7,
    Cancel {
        index: usize,
        begin: usize,
        length: usize,
    } = 8,
}

#[derive(thiserror::Error, Debug)]
pub enum PeerError {
    #[error("Should have exactly 6 bytes in length")]
    UnknownBytesListFormat,
    #[error("Invalid info hash")]
    InvalidInfoHash,
    #[error("Handshake failed, just like in real life")]
    PeerHandshakeFailed,
    #[error("Piece download failed.")]
    DownloadPieceFailed,
    #[error("Peer message is too short. (`{0}`)")]
    PeerMessageTooShort(u32),
    #[error("Peer message is too long. (`{0}`)")]
    PeerMessageTooLong(u32),
    #[error("TcpStream somewhat failed.")]
    TcpStreamConnectionFailure(#[from] std::io::Error),
    #[error("Unexpected garbage values received.")]
    TcpStreamGarbageReceived,
}
