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

use std::{error::Error, io::Read, path::Path};

use bencode::{Bencode, BencodeDictValues};
use sha1_smol::{Digest, Sha1};

// In the single file case, the name key is the name of a file, in the muliple file case, it's the name of a directory.
#[derive(Debug)]
pub struct Torrent {
    pub announce: Option<String>,
    pub info: Info,
}

#[derive(Debug)]
pub struct Info {
    pub file_type: FileType,
    pub name: String,
    pub piece_length: usize,
    pub pieces: Vec<[u8; 20]>,
}

#[derive(Debug)]
pub enum FileType {
    MultiFile { files: Vec<File> },
    SingleFile { length: usize },
}

#[derive(Debug)]
pub struct File {
    pub length: usize,
    pub path: Vec<String>,
}

impl TryFrom<Bencode> for Torrent {
    type Error = TorrentError;

    fn try_from(value: Bencode) -> Result<Self, Self::Error> {
        match value {
            Bencode::Dict(torrent_table) => {
                let announce = torrent_table.get("announce").and_then(|val| match val {
                    BencodeDictValues::Bencode(Bencode::String(s)) => Some(s.clone()),
                    _ => None,
                });

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

impl Torrent {
    pub fn from_file(file_path: &Path) -> Result<Self, Box<dyn Error>> {
        // TODO: Buffered reads?
        let mut f = std::fs::File::open(file_path)?;
        let mut buffer = Vec::new();

        // read the whole file
        f.read_to_end(&mut buffer)?;
        let (values, _) = Bencode::from_bytes(&buffer, |s| match s {
            "pieces" => Some(20),
            _ => None,
        })?;
        Ok(Torrent::try_from(values)?)
    }

    pub fn from_bytes(encoded_bytes: &[u8]) -> Result<Self, Box<dyn Error>> {
        let (values, _) = Bencode::from_bytes(encoded_bytes, |s| match s {
            "pieces" => Some(20),
            _ => None,
        })?;
        Ok(Torrent::try_from(values)?)
    }
}

impl Info {
    pub fn to_bytes(&self) -> Vec<u8> {
        let file_type_bytes = self.file_type.to_bytes();
        let name_bytes = format!("{}:{}", self.name.len(), self.name)
            .as_bytes()
            .to_owned();
        let piece_length_bytes = format!("i{}e", self.piece_length).as_bytes().to_owned();

        let mut res: Vec<u8> = Vec::new();
        res.push(b'd');
        res.extend(file_type_bytes);

        res.extend("4:name".as_bytes());
        res.extend(name_bytes);
        res.extend("12:piece length".as_bytes());
        res.extend(piece_length_bytes);
        res.extend("6:pieces".as_bytes());
        res.extend(format!("{}:", self.pieces.len() * 20).as_bytes().to_owned());
        res.extend(self.pieces.iter().flatten());
        res.push(b'e');
        res
    }
    pub fn get_hash(&self) -> Digest {
        let mut sha1 = Sha1::new();
        sha1.update(&self.to_bytes());
        sha1.digest()
    }

    pub fn get_url_encoded_hash(&self) -> String {
        let hash = hex::encode(self.get_hash().bytes());
        hash.chars()
            .enumerate()
            .fold(String::with_capacity(hash.len()), |mut acc, (i, chr)| {
                if i % 2 == 0 {
                    acc.push('%');
                }
                acc.push(chr);
                acc
            })
    }

    pub fn get_file_length(&self) -> usize {
        match &self.file_type {
            FileType::MultiFile { files } => files.iter().map(|f| f.length).sum(),
            FileType::SingleFile { length } => *length,
        }
    }
}
impl FileType {
    fn to_bytes(&self) -> Vec<u8> {
        match self {
            FileType::MultiFile { files } => [b'l']
                .iter()
                .copied()
                .chain(files.iter().flat_map(|file| {
                    let mut file_vec = format!("6:lengthi{}e", file.length).as_bytes().to_owned();
                    file_vec.extend(
                        file.path
                            .iter()
                            .flat_map(|s| format!("4:path{}:{}", s.len(), s).as_bytes().to_owned()),
                    );
                    file_vec
                }))
                .chain([b'e'].iter().copied())
                .collect(),
            FileType::SingleFile { length } => {
                format!("6:lengthi{}e", length).as_bytes().to_owned()
            }
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

#[derive(Debug)]
pub enum TorrentError {
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
