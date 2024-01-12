mod arg_parse;

use bencode::Bencode;
use bit_torrust::torrent::Torrent;
use std::{
    error::Error,
    io::{Read, Write},
};

use clap::Parser;

fn main() -> Result<(), Box<dyn Error>> {
    let cli = arg_parse::Cli::parse();
    match &cli.action {
        arg_parse::Action::Decode { bencode } => {
            println!("Decoding {}, bytes {:?}", bencode, bencode.as_bytes());
            let (values, _) = Bencode::from_bytes(bencode.as_bytes(), |s| match s {
                "pieces" => Some(20),
                _ => None,
            })?;
            println!("Decoded Bencode = {:?}", values);
            Ok(())
        }
        arg_parse::Action::Info { file } => {
            println!("Decoding File {}", file.display());
            // TODO: Buffered reads?
            let mut f = std::fs::File::open(file)?;
            let mut buffer = Vec::new();

            // read the whole file
            f.read_to_end(&mut buffer)?;
            let (values, _) = Bencode::from_bytes(&buffer, |s| match s {
                "pieces" => Some(20),
                _ => None,
            })?;
            // println!("Decoded Bencode = {:?}", values);
            let torrent_metadata = Torrent::try_from(values)?;
            println!("Decoded Torrent file = {:?}", torrent_metadata);

            println!("Info Hash : {}", torrent_metadata.info.get_hash());
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .write(true)
                .open("res")?;
            let _ = f.write(&torrent_metadata.info.to_bytes());
            Ok(())
        }
    }
}
