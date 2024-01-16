use std::{
    error::Error,
    net::{IpAddr, Ipv4Addr},
    time::{Duration, Instant},
};

use bencode::{Bencode, BencodeDictValues};

const PEER_ID: &str = "1337cafebabedeadbeef";

#[derive(Debug)]
struct Peer {
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

#[derive(Debug)]
enum PeerError {
    UnknownBytesListFormat,
}
impl Error for PeerError {}
impl std::fmt::Display for PeerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PeerError::UnknownBytesListFormat => write!(f, "Should have exactly 6 bytes in length"),
        }
    }
}

#[derive(Debug)]
pub struct TrackerService {
    client: reqwest::blocking::Client,
    interval: Duration,
    tracker_url: String,
    peers: Vec<Peer>,
    last_updated: Instant,
    port: u16,
    encoded_info_hash: String,
}

impl TrackerService {
    pub fn new(url: &str, port: u16, url_encoded_hash: &str) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            interval: Duration::default(),
            tracker_url: url.to_string(),
            peers: Vec::new(),
            last_updated: Instant::now(),
            port,
            encoded_info_hash: url_encoded_hash.to_string(),
        }
    }
    pub fn update(
        &mut self,
        uploaded: u64,
        downloaded: u64,
        left: u64,
    ) -> Result<(), Box<dyn Error>> {
        let query_params = [
            ("peer_id", PEER_ID),
            ("port", &self.port.to_string()),
            ("uploaded", &uploaded.to_string()),
            ("downloaded", &downloaded.to_string()),
            ("left", &left.to_string()),
            ("compact", "1"),
        ];
        let request = self
            .client
            // NOTE: Just take the url encoded hash AS IS, don't do anything smart like
            // treating valid characters as not needing to be escaped.
            .get(format!(
                "{}?info_hash={}",
                self.tracker_url, self.encoded_info_hash
            ))
            .query(&query_params);

        let response = request.send()?.bytes()?;
        let (bencoded_response, _) = Bencode::from_bytes(&response, |s| match s {
            "peers" => Some(6),
            _ => None,
        })?;
        if let Bencode::Dict(table) = bencoded_response {
            if let Some(BencodeDictValues::Bencode(Bencode::Number(n))) = table.get("interval") {
                self.interval = Duration::from_secs(*n as u64);
            }
            if let Some(BencodeDictValues::Bytes(peer_table)) = table.get("peers") {
                self.peers.clear();
                self.peers.extend(
                    peer_table
                        .iter()
                        .map(|peer| TryInto::<Peer>::try_into(peer.as_slice()).unwrap()),
                );
            }
        }

        Ok(())
    }
}
