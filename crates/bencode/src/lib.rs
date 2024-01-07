use std::error::Error;

use indexmap::IndexMap;

pub type BenResult<T> = Result<T, Box<dyn Error>>;
#[derive(Debug, PartialEq, Eq)]
pub enum Bencode {
    String(String),
    Number(i64),
    List(Vec<Bencode>),
    Dict(IndexMap<String, BencodeDictValues>),
}

impl Bencode {
    pub fn to_bytes(&self) -> BenResult<Vec<u8>> {
        match self {
            Bencode::String(s) => Ok(format!("{}:{}", s.len(), s).as_bytes().to_owned()),
            Bencode::Number(i) => Ok(format!("i{}e", i).as_bytes().to_owned()),
            Bencode::List(ben_vec) => {
                let mut res: Vec<_> = vec![b'l'];
                ben_vec.iter().try_for_each(|bencode| -> BenResult<()> {
                    res.extend(bencode.to_bytes()?);
                    Ok(())
                })?;
                res.push(b'e');
                Ok(res)
            }
            Bencode::Dict(ben_map) => {
                let mut res: Vec<_> = vec![b'd'];
                ben_map
                    .iter()
                    .try_for_each(|(key, value)| -> BenResult<()> {
                        // NOTE: A little code-dupe goes a long way~
                        res.extend(format!("{}:{}", key.len(), key).as_bytes().to_owned());
                        match value {
                            BencodeDictValues::Bencode(bencode) => {
                                res.extend(bencode.to_bytes()?);
                            }
                            BencodeDictValues::Bytes(bytez) => {
                                let byte_size: usize =
                                    bytez.iter().map(|byte_arr| byte_arr.len()).sum();
                                res.extend(format!("{}:", byte_size).as_bytes());
                                bytez.iter().for_each(|v| res.extend(v))
                            }
                        }
                        Ok(())
                    })?;
                res.push(b'e');
                Ok(res)
            }
        }
    }

    pub fn from_bytes(
        encoded_value: &[u8],
        byte_mode_key: fn(&str) -> Option<usize>,
    ) -> BenResult<(Self, &[u8])> {
        match encoded_value[0] as char {
            x if x.is_ascii_digit() => Self::bendecode_s(encoded_value),
            'i' => Self::bendecode_i(&encoded_value[1..]),
            'l' => Self::bendecode_l(&encoded_value[1..], byte_mode_key),
            'd' => Self::bendecode_d(&encoded_value[1..], byte_mode_key),
            'e' => Err(Box::new(BenError::MisplacedClosingError)),
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
        let string =
            std::str::from_utf8(&encoded_value[colon_index + 1..colon_index + 1 + length])?;
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
            let (val, returned) = Bencode::from_bytes(rem, byte_mode_key)?;
            list.push(val);
            rem = returned;
        }
        Ok((Bencode::List(list), &rem[1..]))
    }

    fn bendecode_bytez(
        encoded_value: &[u8],
        chunk_size: usize,
    ) -> BenResult<(Vec<Vec<u8>>, &[u8])> {
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
            let (key, returned) = Self::bendecode_s(rem)?;
            // NOTE: This is impossible to fail if the above did not return
            if let Bencode::String(s) = key {
                match byte_mode_key(&s) {
                    None => {
                        let (val, returned) = Bencode::from_bytes(returned, byte_mode_key)?;
                        dict.insert(s, BencodeDictValues::Bencode(val));
                        rem = returned;
                    }
                    Some(chunk_size) => {
                        let (val, returned) = Self::bendecode_bytez(returned, chunk_size)?;
                        dict.insert(s, BencodeDictValues::Bytes(val));
                        rem = returned;
                    }
                }
            }
        }
        Ok((Bencode::Dict(dict), &rem[1..]))
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum BencodeDictValues {
    Bencode(Bencode),
    // Special case of Bencode, some number of raw bytes.
    Bytes(Vec<Vec<u8>>),
}

#[derive(Debug)]
pub enum BenError {
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
