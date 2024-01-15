mod arg_parse;

use bit_torrust::torrent::Torrent;
use std::error::Error;

use clap::Parser;

const PEER_ID: &str = "1337cafebabedeadbeef";

fn main() -> Result<(), Box<dyn Error>> {
    let cli = arg_parse::Cli::parse();
    match &cli.action {
        arg_parse::Action::Decode { bencode } => {
            let torrent_metadata = Torrent::from_bytes(bencode.as_bytes())?;
            println!("Decoded Bencode = {:?}", torrent_metadata);
            Ok(())
        }
        arg_parse::Action::Info { file } => {
            let torrent_metadata = Torrent::from_file(file)?;

            // Tracker GET
            // Well if there are no announce, we'll just not handle it for now.
            let hash = hex::encode(torrent_metadata.info.get_hash().bytes());
            let url_encoded_hash = hash.chars().enumerate().fold(
                String::with_capacity(hash.len()),
                |mut acc, (i, chr)| {
                    if i % 2 == 0 {
                        acc.push('%');
                    }
                    acc.push(chr);
                    acc
                },
            );

            let info_table = &torrent_metadata.info;
            let left = (info_table.piece_length * info_table.pieces.len()) as u64;
            let query_params = [
                ("peer_id", PEER_ID),
                ("port", "6881"),
                ("uploaded", "0"),
                ("downloaded", "0"),
                ("left", &left.to_string()),
                ("compact", "1"),
            ];

            let client = reqwest::blocking::Client::new();
            let request = client
                // NOTE: Just take the url encoded hash AS IS, don't do anything smart like
                // treating valid characters as not needing to be escaped.
                .get(format!(
                    "{}?info_hash={}",
                    &torrent_metadata.announce.unwrap(),
                    url_encoded_hash
                ))
                .query(&query_params);
            println!("Request is {:?}", request);

            let response = request.send()?;
            println!("Tracker request body {}", response.text()?);
            Ok(())
        }
    }
}
