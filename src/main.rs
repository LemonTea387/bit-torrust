mod arg_parse;

use indexmap::IndexMap;
use std::{error::Error, io::Read};

use clap::Parser;

// Metainfo files (also known as .torrent files) are bencoded dictionaries with the following keys:

// announce
//     The URL of the tracker.
// info
//     This maps to a dictionary, with keys described below.

// All strings in a .torrent file that contains text must be UTF-8 encoded.
// info dictionary

// The name key maps to a UTF-8 encoded string which is the suggested name to save the file (or directory) as. It is purely advisory.

// piece length maps to the number of bytes in each piece the file is split into. For the purposes of transfer, files are split into fixed-size pieces which are all the same length except for possibly the last one which may be truncated. piece length is almost always a power of two, most commonly 2 18 = 256 K (BitTorrent prior to version 3.2 uses 2 20 = 1 M as default).

// pieces maps to a string whose length is a multiple of 20. It is to be subdivided into strings of length 20, each of which is the SHA1 hash of the piece at the corresponding index.

// There is also a key length or a key files, but not both or neither. If length is present then the download represents a single file, otherwise it represents a set of files which go in a directory structure.

// In the single file case, length maps to the length of the file in bytes.

// For the purposes of the other keys, the multi-file case is treated as only having a single file by concatenating the files in the order they appear in the files list. The files list is the value files maps to, and is a list of dictionaries containing the following keys:

// length - The length of the file, in bytes.

// path - A list of UTF-8 encoded strings corresponding to subdirectory names, the last of which is the actual file name (a zero length list is an error case).

// In the single file case, the name key is the name of a file, in the muliple file case, it's the name of a directory.
type BenResult<T> = Result<T, Box<dyn Error>>;
struct Torrent {
    announce: reqwest::Url,
    info: Info,
}

struct Info {
    file_type: FileType,
    name: String,
    piece_length: usize,
    pieces: Vec<[u8; 20]>,
}

enum FileType {
    MultiFile { files: Vec<File> },
    SingleFile { length: usize },
}

struct File {
    length: usize,
    path: Vec<String>,
}

#[derive(Debug, PartialEq, Eq)]
enum Bencode {
    String(String),
    Number(i64),
    List(Vec<Bencode>),
    Dict(IndexMap<String, BencodeDictValues>),
}

#[derive(Debug, PartialEq, Eq)]
enum BencodeDictValues {
    Bencode(Bencode),
    // Special case of Bencode, some number of raw bytes.
    Bytes(Vec<Vec<u8>>),
}

#[derive(Debug)]
enum BenError {
    MisplacedClosingError,
    UnexpectedTruncationError,
    UnexpectedToken { token: u8 },
    MissingToken { token: u8 },
}

impl std::error::Error for BenError {}
impl std::fmt::Display for BenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BenError::MisplacedClosingError => {
                write!(f, "Unexpected closing tag 'e' found.")
            }
            BenError::UnexpectedTruncationError => write!(f, "Unexpected ending of byte stream."),
            BenError::UnexpectedToken { token } => {
                write!(f, "Unexpected token : {}.", token)
            }
            BenError::MissingToken { token } => {
                write!(f, "Missing token in stream : {}.", token)
            }
        }
    }
}

fn decode_bencoded_value(
    encoded_value: &[u8],
    byte_mode_key: fn(&str) -> Option<usize>,
) -> BenResult<(Bencode, &[u8])> {
    match encoded_value[0] as char {
        x if x.is_ascii_digit() => bendecode_s(encoded_value),
        'i' => bendecode_i(&encoded_value[1..]),
        'l' => bendecode_l(&encoded_value[1..], byte_mode_key),
        'd' => bendecode_d(&encoded_value[1..], byte_mode_key),
        x => Err(Box::new(BenError::UnexpectedToken { token: x as u8 })),
    }
}

fn bendecode_s(encoded_value: &[u8]) -> BenResult<(Bencode, &[u8])> {
    // Iterate through and find the character that matches ':'
    let colon_index = encoded_value
        .iter()
        .position(|&x| x == b':')
        .ok_or(BenError::MissingToken { token: b':' })?;
    let length_string = std::str::from_utf8(&encoded_value[..colon_index])?;
    let length = length_string.parse::<usize>()?;
    let string = std::str::from_utf8(&encoded_value[colon_index + 1..colon_index + 1 + length])?;
    Ok((
        Bencode::String(string.to_string()),
        &encoded_value[colon_index + 1 + length..],
    ))
}

fn bendecode_i(encoded_value: &[u8]) -> BenResult<(Bencode, &[u8])> {
    let ending_index = encoded_value
        .iter()
        .position(|&x| x == b'e')
        .ok_or(BenError::MissingToken { token: b'e' })?;
    let i_string = std::str::from_utf8(&encoded_value[..ending_index])?;
    let number = i_string.parse::<i64>().unwrap();
    Ok((Bencode::Number(number), &encoded_value[ending_index + 1..]))
}

fn bendecode_l(
    encoded_value: &[u8],
    byte_mode_key: fn(&str) -> Option<usize>,
) -> BenResult<(Bencode, &[u8])> {
    let mut list = Vec::new();
    let mut rem = encoded_value;
    while !rem.is_empty() && rem[0] != b'e' {
        let (val, returned) = decode_bencoded_value(rem, byte_mode_key)?;
        list.push(val);
        rem = returned;
    }
    Ok((Bencode::List(list), &rem[1..]))
}

fn bendecode_bytez(encoded_value: &[u8], chunk_size: usize) -> BenResult<(Vec<Vec<u8>>, &[u8])> {
    // Iterate through and find the character that matches ':'
    let colon_index = encoded_value
        .iter()
        .position(|&x| x == b':')
        .ok_or(BenError::MissingToken { token: b':' })?;
    let length_string = std::str::from_utf8(&encoded_value[..colon_index])?;
    let length = length_string.parse::<usize>()?;
    // Length should be a multiple of chunk_size!
    if length % chunk_size != 0 {
        return Err(Box::new(BenError::UnexpectedTruncationError));
    }

    let bytes = &encoded_value[colon_index + 1..colon_index + 1 + length];
    Ok((
        bytes
            .chunks(chunk_size)
            .map(|x| x.to_vec())
            .collect::<Vec<Vec<u8>>>(),
        &encoded_value[colon_index + 1 + length..],
    ))
}

fn bendecode_d(
    encoded_value: &[u8],
    byte_mode_key: fn(&str) -> Option<usize>,
) -> BenResult<(Bencode, &[u8])> {
    // We know that they must be strings
    let mut dict = IndexMap::new();
    let mut rem = encoded_value;
    while !rem.is_empty() && rem[0] != b'e' {
        let (key, returned) = bendecode_s(rem)?;
        // NOTE: This is impossible to fail if the above did not return
        if let Bencode::String(s) = key {
            match byte_mode_key(&s) {
                None => {
                    let (val, returned) = decode_bencoded_value(returned, byte_mode_key)?;
                    dict.insert(s, BencodeDictValues::Bencode(val));
                    rem = returned;
                }
                Some(chunk_size) => {
                    let (val, returned) = bendecode_bytez(returned, chunk_size)?;
                    dict.insert(s, BencodeDictValues::Bytes(val));
                    rem = returned;
                }
            }
        }
    }
    Ok((Bencode::Dict(dict), &rem[1..]))
}

fn main() -> BenResult<()> {
    let cli = arg_parse::Cli::parse();
    match &cli.action {
        arg_parse::Action::Decode { bencode } => {
            println!("Decoding {}, bytes {:?}", bencode, bencode.as_bytes());
            let (values, _) = decode_bencoded_value(bencode.as_bytes(), |s| match s {
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
            let (values, _) = decode_bencoded_value(&buffer, |s| match s {
                "pieces" => Some(20),
                _ => None,
            })?;
            println!("Decoded Bencode = {:?}", values);
            Ok(())
        }
    }
}
