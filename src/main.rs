mod arg_parse;

use bencode::{Bencode, BencodeDictValues};
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
#[derive(Debug)]
struct Torrent {
    announce: Option<reqwest::Url>,
    info: Info,
}

#[derive(Debug)]
struct Info {
    file_type: FileType,
    name: String,
    piece_length: usize,
    pieces: Vec<[u8; 20]>,
}

#[derive(Debug)]
enum FileType {
    MultiFile { files: Vec<File> },
    SingleFile { length: usize },
}

#[derive(Debug)]
struct File {
    length: usize,
    path: Vec<String>,
}

impl TryFrom<Bencode> for Torrent {
    type Error = TorrentError;

    fn try_from(value: Bencode) -> Result<Self, Self::Error> {
        match value {
            Bencode::Dict(torrent_table) => {
                let announce = torrent_table.get("announce").and_then(|val| match val {
                    BencodeDictValues::Bencode(Bencode::String(s)) => Some(s),
                    _ => None,
                });
                let announce = match announce {
                    Some(s) => {
                        Some(reqwest::Url::parse(s).map_err(|_| TorrentError::InvalidAnnounceUrl)?)
                    }
                    None => None,
                };

                let info = match torrent_table.get("info") {
                    Some(BencodeDictValues::Bencode(info_table)) => Info::parse_info(info_table),
                    _ => Err(TorrentError::InvalidTorrentFile(
                        "Info dictionary does not exist.".to_string(),
                    )),
                }?;

                Ok(Self { announce, info })
            }
            _ => Err(TorrentError::InvalidTorrentFile(
                "Torrent metainfo file should have a bencoded dictionary.".to_string(),
            )),
        }
    }
}

impl Info {
    fn parse_info(value: &Bencode) -> Result<Self, TorrentError> {
        let info_table = match value {
            Bencode::Dict(val) => val,
            _ => {
                return Err(TorrentError::InvalidTorrentFile(
                    "Files list is not a valid bencoded dictionary.".to_string(),
                ))
            }
        };
        let file_type = Self::resolve_file_type(value)?;
        let name = info_table
            .get("name")
            .and_then(|val| match val {
                BencodeDictValues::Bencode(Bencode::String(s)) => Some(s.to_string()),
                _ => None,
            })
            .ok_or(TorrentError::InvalidTorrentFile(
                "Should have advisory name".to_string(),
            ))?;

        let piece_length = info_table
            .get("piece length")
            .and_then(|val| match val {
                BencodeDictValues::Bencode(Bencode::Number(i)) => Some(*i as usize),
                _ => None,
            })
            .ok_or(TorrentError::InvalidTorrentFile(
                "Should have piece length information.".to_string(),
            ))?;

        let pieces = match info_table.get("pieces") {
            Some(BencodeDictValues::Bytes(bytez)) => {
                let mut result: Vec<[u8; 20]> = Vec::new();
                bytez.into_iter().try_for_each(|vec_of_bytes| {
                    if vec_of_bytes.len() != 20 {
                        return Err(TorrentError::InvalidTorrentFile(
                            "Invalid file hash.".to_string(),
                        ));
                    }
                    // This unwrap and the indexing is safe as it is hopefully handled at the top.
                    result.push(<[u8; 20]>::try_from(&vec_of_bytes[..20]).unwrap());
                    Ok(())
                })?;
                Ok(result)
            }
            _ => Err(TorrentError::InvalidTorrentFile(
                "No pieces found".to_string(),
            )),
        }?;

        Ok(Self {
            file_type,
            name,
            piece_length,
            pieces,
        })
    }
    fn resolve_file_type(value: &Bencode) -> Result<FileType, TorrentError> {
        let info_table = match value {
            Bencode::Dict(val) => val,
            _ => {
                return Err(TorrentError::InvalidTorrentFile(
                    "Files list is not a valid bencoded dictionary.".to_string(),
                ))
            }
        };
        let file_type;
        // Check file mode
        if let Some(BencodeDictValues::Bencode(Bencode::Number(x))) = info_table.get("length") {
            file_type = FileType::SingleFile {
                length: *x as usize,
            };
        } else if let Some(BencodeDictValues::Bencode(Bencode::List(files_list))) =
            info_table.get("files")
        {
            let files = files_list
                .into_iter()
                .map(|bencode| {
                    // WARNING: PREPARE FOR SOME CODE ABOMINATION
                    // File list contains dictionary representing a File
                    match bencode {
                        Bencode::Dict(file_table) => {
                            let length = match file_table.get("length") {
                                Some(BencodeDictValues::Bencode(Bencode::Number(x))) => *x as usize,
                                _ => {
                                    return Err(TorrentError::InvalidTorrentFile(
                                        "File does not have valid file length.".to_string(),
                                    ))
                                }
                            };
                            let path = match file_table.get("path") {
                                Some(BencodeDictValues::Bencode(Bencode::List(list_of_path))) => {
                                    // We pray that list_of_path is actually list of strings.
                                    list_of_path
                                        .iter()
                                        .map(|bencode| match bencode {
                                            Bencode::String(s) => Ok(s.to_string()),
                                            _ => Err(TorrentError::InvalidTorrentFile(
                                                "Invalid file path".to_string(),
                                            )),
                                        })
                                        .collect::<Result<Vec<String>, TorrentError>>()
                                }
                                _ => {
                                    return Err(TorrentError::InvalidTorrentFile(
                                        "File does not have a valid file path.".to_string(),
                                    ))
                                }
                            }?;
                            Ok(File { length, path })
                        }
                        _ => Err(TorrentError::InvalidTorrentFile(
                            "Invalid files list.".to_string(),
                        )),
                    }
                })
                .collect::<Result<Vec<_>, TorrentError>>()?;
            file_type = FileType::MultiFile { files }
        } else {
            return Err(TorrentError::InvalidTorrentFile(
                "Could not determine file type".to_string(),
            ));
        }

        Ok(file_type)
    }
}

impl TryFrom<Bencode> for Info {
    type Error = TorrentError;

    fn try_from(value: Bencode) -> Result<Self, Self::Error> {
        // Determine if it's a multifile or singlefile, a singlefile only has length
        if let Bencode::Dict(info_table) = value {}
        Err(TorrentError::InvalidTorrentFile(
            "Info table should have a bencoded dictionary.".to_string(),
        ))
    }
}

#[derive(Debug)]
enum TorrentError {
    InvalidAnnounceUrl,
    InvalidTorrentFile(String),
}
impl std::error::Error for TorrentError {}

impl std::fmt::Display for TorrentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TorrentError::InvalidTorrentFile(s) => {
                write!(f, "Not a valid torrent file, missing {}", s)
            }
            TorrentError::InvalidAnnounceUrl => todo!(),
        }
    }
}

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
            println!("Decoded Bencode = {:?}", values);
            println!("Decoded Torrent file = {:?}", Torrent::try_from(values)?);
            Ok(())
        }
    }
}
