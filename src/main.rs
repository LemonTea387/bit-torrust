mod arg_parse;

use std::{collections::HashMap, error::Error};

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
    Dict(HashMap<String, Bencode>),
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

fn bendecode_s(encoded_value: &str) -> (String, &str) {
    let colon_index = encoded_value.find(':').unwrap();
    let number_string = &encoded_value[..colon_index];
    let number = number_string.parse::<u32>().unwrap();
    let string = &encoded_value[colon_index + 1..colon_index + 1 + number as usize];
    (
        string.to_string(),
        &encoded_value[colon_index + 1 + number as usize..],
    )
}

fn bendecode_i(encoded_value: &str) -> (i64, &str) {
    // NOTE: Skip 'i'
    let ending_index = encoded_value.find('e').unwrap();
    let i_string = &encoded_value[1..ending_index];
    let number = i_string.parse::<i64>().unwrap();
    (number, &encoded_value[ending_index + 1..])
}

fn bendecode_l(encoded_value: &str) -> (Vec<serde_json::Value>, &str) {
    let mut list = Vec::new();

    let mut rem = encoded_value.split_at(1).1;
    while !rem.is_empty() && !rem.starts_with('e') {
        let (val, returned) = decode_bencoded_value(rem);
        list.push(val);
        rem = returned;
    }
    (list, rem.strip_prefix('e').unwrap())
}

fn bendecode_d(encoded_value: &str) -> (Map<String, serde_json::Value>, &str) {
    // We know that they must be strings.
    let mut dict = Map::new();
    let mut rem = encoded_value.split_at(1).1;
    while !rem.is_empty() && !rem.starts_with('e') {
        let (key, returned) = bendecode_s(rem);
        let (val, returned) = decode_bencoded_value(returned);
        dict.insert(key, val);
        rem = returned;
    }
    (dict, rem.strip_prefix('e').unwrap())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let command = &args[1];

    if command == "decode" {
        let encoded_value = &args[2];
        let (decoded_value, _) = decode_bencoded_value(encoded_value);
        println!("{}", decoded_value);
    } else {
        eprintln!("unknown command: {}", args[1])
    }
}
