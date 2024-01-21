mod arg_parse;

use bit_torrust::{torrent::Torrent, tracker::TrackerService};
use std::error::Error;

use clap::Parser;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = arg_parse::Cli::parse();
    match &cli.action {
        arg_parse::Action::Decode { bencode } => {
            let torrent_metadata = Torrent::from_bytes(bencode.as_bytes())?;
            println!("Decoded Bencode = {:?}", torrent_metadata);
            Ok(())
        }
        arg_parse::Action::Info {
            file,
            peer_discovery,
        } => {
            let torrent_metadata = Torrent::from_file(file)?;
            if *peer_discovery {
                // Tracker GET
                // Well if there are no announce, we'll just not handle it for now.
                let info_table = &torrent_metadata.info;
                let url_encoded_hash = info_table.get_url_encoded_hash();
                let left = (info_table.piece_length * info_table.pieces.len()) as u64;
                let mut tracker_service = TrackerService::new(
                    &torrent_metadata.announce.unwrap(),
                    6881,
                    &url_encoded_hash,
                );
                let peers = tracker_service.get_peers(0, 0, left)?;
                println!(
                    "Peers : \n{}",
                    peers
                        .into_iter()
                        .map(|peer| peer.to_string())
                        .collect::<Vec<String>>()
                        .join("\n")
                );
            }
            Ok(())
        }
        arg_parse::Action::Download { file } => {
            let torrent_metadata = Torrent::from_file(file)?;
            let info_table = &torrent_metadata.info;
            let url_encoded_hash = info_table.get_url_encoded_hash();
            let left = (info_table.piece_length * info_table.pieces.len()) as u64;
            let mut tracker_service =
                TrackerService::new(&torrent_metadata.announce.unwrap(), 6881, &url_encoded_hash);
            let peers = tracker_service.get_peers(0, 0, left)?;
            let hash = info_table.get_hash().bytes();
            // TODO: Maintain a pool of connections to peers, 5 as per recommended in spec of
            // bittorrent economics paper
            let connection = peers[0].connect(&hash)?;
            let connection = connection.handshake()?;

            Ok(())
        }
    }
}
