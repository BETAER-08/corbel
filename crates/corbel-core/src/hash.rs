use crate::error::{Error, Result};
use std::fmt;
use std::fs::File;
use std::io::Read;
use std::path::Path;
use xxhash_rust::xxh3::Xxh3;

const BUFFER_SIZE: usize = 64 * 1024;

#[derive(Copy, Clone, PartialEq, Eq, Hash, Debug)]
pub struct ContentHash(u64);

impl ContentHash {
    pub fn from_hex(s: &str) -> Result<Self> {
        if s.len() != 16 {
            return Err(Error::InvalidContentHash {
                value: s.to_string(),
            });
        }
        let value = u64::from_str_radix(s, 16).map_err(|_| Error::InvalidContentHash {
            value: s.to_string(),
        })?;
        Ok(ContentHash(value))
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.0)
    }
}

pub fn hash_file(path: impl AsRef<Path>) -> Result<ContentHash> {
    let path = path.as_ref();
    let mut file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = Xxh3::new();
    let mut buffer = [0u8; BUFFER_SIZE];
    loop {
        let bytes_read = file.read(&mut buffer).map_err(|e| Error::io(path, e))?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }
    Ok(ContentHash(hasher.digest()))
}

pub fn hash_bytes(data: &[u8]) -> ContentHash {
    ContentHash(xxhash_rust::xxh3::xxh3_64(data))
}
