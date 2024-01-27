mod arg_parse;

use bit_torrust::{torrent::Torrent, tracker::TrackerService};
use std::{error::Error, fs::File, io::Write};

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
                let mut tracker_service = TrackerService::new(6881, &torrent_metadata);
                let peers = tracker_service.get_peers(
                    0,
                    0,
                    torrent_metadata.info.get_file_length() as u64,
                )?;
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
        // NOTE: Currently downloads single file torrent only in sequence
        arg_parse::Action::Download { file: torrent_file } => {
            let torrent_metadata = Torrent::from_file(torrent_file)?;
            let hash = torrent_metadata.info.get_hash().bytes();
            let mut tracker_service = TrackerService::new(6881, &torrent_metadata);
            let peers =
                tracker_service.get_peers(0, 0, torrent_metadata.info.get_file_length() as u64)?;
            // TODO: Maintain a pool of connections to peers
            let mut connection = peers[0].connect(&torrent_metadata.info, &hash)?;
            let mut pieces = Vec::new();
            for i in 0..torrent_metadata.info.pieces.len() {
                println!("Downloading piece {i}");
                pieces.push(connection.download_piece(i as u32)?);
            }
            let mut file = File::create(&torrent_metadata.info.name)?;
            println!("Saved to {}", &torrent_metadata.info.name);
            for piece in pieces.into_iter() {
                file.write_all(&piece.piece)?;
            }
            Ok(())
        }
    }
}
