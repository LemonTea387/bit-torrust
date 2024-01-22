use std::{
    error::Error,
    io::{Read, Write},
    net::{IpAddr, Ipv4Addr, TcpStream},
};

use byteorder::{BigEndian, ByteOrder, ReadBytesExt};

use crate::torrent::Info;

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
    pub fn connect<'a>(
        &'a self,
        info_table: &'a Info,
        info_hash: &'a [u8],
    ) -> Result<PeerConnection, Box<dyn Error>> {
        let connection = TcpStream::connect((self.ip_addr, self.port))?;
        PeerConnection::new(connection, info_table, info_hash)
    }
}

impl std::fmt::Display for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.ip_addr, self.port)
    }
}

pub struct PeerConnection<'a> {
    connection: TcpStream,
    info_table: &'a Info,
    peer_id: PeerId,
}

impl PeerConnection<'_> {
    pub fn new<'a>(
        mut connection: TcpStream,
        info_table: &'a Info,
        info_hash: &'a [u8],
    ) -> Result<PeerConnection<'a>, Box<dyn Error>> {
        let peer_id = Self::handshake(&mut connection, info_hash)?;
        Ok(PeerConnection {
            connection,
            info_table,
            peer_id,
        })
    }
    fn handshake(connection: &mut TcpStream, info_hash: &[u8]) -> Result<PeerId, PeerError> {
        // Try handshake
        // <19 in byte>BitTorrent protocol<8Bytes0><20byte sha1 info table hash><20peerid>
        let mut buf = [0u8; 68];
        buf[0] = 19;
        buf[1..20].clone_from_slice(b"BitTorrent protocol");
        buf[28..48].clone_from_slice(info_hash);
        buf[48..].clone_from_slice(PEER_ID.as_bytes());

        match connection.write(&buf) {
            Ok(68) => {}
            _ => return Err(PeerError::PeerHandshakeFailed),
        }

        let mut response_buf = [0u8; 68];
        connection.read_exact(&mut response_buf)?;

        let mut peer_id = [0u8; 20];
        peer_id.clone_from_slice(&response_buf[48..]);
        Ok(PeerId(
            response_buf[48..]
                .try_into()
                .expect("Slice should already have the right length!"),
        ))
    }
    fn receive_decode(&mut self) -> Result<Option<PeerMessage>, PeerError> {
        // Length is a 4byte int:msgcode[payload]
        let mut len_buf = [0u8; 4];
        self.connection.read_exact(&mut len_buf)?;
        let len = BigEndian::read_u32(&len_buf);
        if len == 0 {
            // Just keep-alive, go next
            return Ok(None);
        }
        let msg_type = self.connection.read_u8()?;
        // The actual msg len does not count the msg code
        let actual_msg_len = len - 1;
        match msg_type {
            0 => Ok(Some(PeerMessage::Choke)),
            1 => Ok(Some(PeerMessage::Unchoke)),
            2 => Ok(Some(PeerMessage::Interested)),
            3 => Ok(Some(PeerMessage::NotInterested)),
            4 => Ok(Some(PeerMessage::Have)),
            //TODO : Check length with the received stuff.
            5 => {
                let mut bitfield = vec![0u8; actual_msg_len as usize];
                self.connection.read_exact(&mut bitfield)?;
                Ok(Some(PeerMessage::Bitfield(bitfield)))
            }
            6 => Ok(Some(PeerMessage::Request {
                index: self.connection.read_u32::<BigEndian>()?,
                begin: self.connection.read_u32::<BigEndian>()?,
                length: self.connection.read_u32::<BigEndian>()?,
            })),
            7 => {
                let index = self.connection.read_u32::<BigEndian>()?;
                let begin = self.connection.read_u32::<BigEndian>()?;
                let mut piece_data = vec![0u8; (actual_msg_len - 8) as usize];
                self.connection.read_exact(&mut piece_data)?;
                Ok(Some(PeerMessage::Piece {
                    index,
                    begin,
                    piece: piece_data,
                }))
            }
            8 => Ok(Some(PeerMessage::Cancel {
                index: self.connection.read_u32::<BigEndian>()?,
                begin: self.connection.read_u32::<BigEndian>()?,
                length: self.connection.read_u32::<BigEndian>()?,
            })),
            _ => Err(PeerError::TcpStreamGarbageReceived),
        }
    }
}

struct PeerId([u8; 20]);

#[repr(u8)]
enum PeerMessage {
    Choke = 0,
    Unchoke = 1,
    Interested = 2,
    NotInterested = 3,
    Have = 4,
    Bitfield(Vec<u8>) = 5,
    Request {
        index: u32,
        begin: u32,
        length: u32,
    } = 6,
    Piece {
        index: u32,
        begin: u32,
        piece: Vec<u8>,
    } = 7,
    Cancel {
        index: u32,
        begin: u32,
        length: u32,
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
