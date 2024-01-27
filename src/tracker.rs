use crate::{
    peer::{Peer, PEER_ID},
    torrent::Torrent,
};
use std::{
    error::Error,
    time::{Duration, Instant},
};

use bencode::{Bencode, BencodeDictValues};

#[derive(Debug)]
pub struct TrackerService<'a> {
    client: reqwest::blocking::Client,
    interval: Duration,
    last_updated: Instant,
    port: u16,
    torrent: &'a Torrent,
}

impl<'a> TrackerService<'a> {
    pub fn new(port: u16, torrent: &'a Torrent) -> Self {
        Self {
            client: reqwest::blocking::Client::new(),
            interval: Duration::default(),
            last_updated: Instant::now(),
            port,
            torrent,
        }
    }

    pub fn get_peers(
        &mut self,
        uploaded: u64,
        downloaded: u64,
        left:u64
    ) -> Result<Vec<Peer>, Box<dyn Error>> {
        let query_params = [
            ("peer_id", PEER_ID),
            ("port", &self.port.to_string()),
            ("uploaded", &uploaded.to_string()),
            ("downloaded", &downloaded.to_string()),
            ("left", &left.to_string()),
            ("compact", "1"),
        ];

        let tracker_url = self
            .torrent
            .announce
            .as_ref()
            .expect("Annouce should be present");
        let encoded_info_hash = self.torrent.info.get_url_encoded_hash();

        let request = self
            .client
            // NOTE: Just take the url encoded hash AS IS, don't do anything smart like
            // treating valid characters as not needing to be escaped.
            .get(format!("{}?info_hash={}", tracker_url, encoded_info_hash))
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
            match table.get("peers") {
                Some(BencodeDictValues::Bytes(peer_table)) => {
                    return Ok(peer_table
                        .iter()
                        .map(|peer| TryInto::<Peer>::try_into(peer.as_slice()).unwrap())
                        .collect())
                }
                _ => return Err(Box::new(TrackerError::MalformedTrackerResponse)),
            }
        }
        Err(Box::new(TrackerError::MalformedTrackerResponse))
    }
}

#[derive(Debug)]
pub enum TrackerError {
    MalformedTrackerResponse,
}

impl std::error::Error for TrackerError {}
impl std::fmt::Display for TrackerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TrackerError::MalformedTrackerResponse => write!(f, "Malformed Tracker Response!"),
        }
    }
}
