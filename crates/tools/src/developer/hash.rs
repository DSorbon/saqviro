//! Hashing, shared by Hash Generator and the file checksum tools.

use std::io::{self, Read};

use md5::Md5;
use saqviro_jobs::Params;
use sha1::Sha1;
use sha2::{Digest, Sha256, Sha512};

use crate::params;

const CHUNK: usize = 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    Md5,
    Sha1,
    Sha256,
    Sha512,
    Blake3,
}

impl Algorithm {
    pub const ALL: [Self; 5] = [Self::Md5, Self::Sha1, Self::Sha256, Self::Sha512, Self::Blake3];

    pub fn name(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
            Self::Blake3 => "BLAKE3",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|a| a.name().replace('-', "").eq_ignore_ascii_case(id))
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn with_digest<D: Digest>(
    source: &mut impl Read,
    on_chunk: &mut impl FnMut(u64) -> io::Result<()>,
) -> io::Result<String> {
    let mut digest = D::new();
    read_chunks(source, on_chunk, |chunk| digest.update(chunk))?;
    Ok(hex(&digest.finalize()))
}

fn read_chunks(
    source: &mut impl Read,
    on_chunk: &mut impl FnMut(u64) -> io::Result<()>,
    mut update: impl FnMut(&[u8]),
) -> io::Result<()> {
    let mut buffer = vec![0; CHUNK];
    loop {
        let n = source.read(&mut buffer)?;
        if n == 0 {
            return Ok(());
        }
        update(&buffer[..n]);
        on_chunk(n as u64)?;
    }
}

/// Hashes `source` piece by piece. `on_chunk` gets the size of each piece
/// and can stop the work by returning an error.
pub fn hash(
    algorithm: Algorithm,
    source: &mut impl Read,
    mut on_chunk: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<String> {
    let chunks = &mut on_chunk;
    match algorithm {
        Algorithm::Md5 => with_digest::<Md5>(source, chunks),
        Algorithm::Sha1 => with_digest::<Sha1>(source, chunks),
        Algorithm::Sha256 => with_digest::<Sha256>(source, chunks),
        Algorithm::Sha512 => with_digest::<Sha512>(source, chunks),
        Algorithm::Blake3 => {
            let mut hasher = blake3::Hasher::new();
            read_chunks(source, chunks, |chunk| {
                hasher.update(chunk);
            })?;
            Ok(hasher.finalize().to_hex().to_string())
        }
    }
}

pub fn hash_text(algorithm: Algorithm, text: &str) -> String {
    hash(algorithm, &mut text.as_bytes(), |_| Ok(())).expect("reading from memory cannot fail")
}

/// Hash Generator: one algorithm, or all of them.
pub fn run(text: &str, params: &Params) -> Result<String, String> {
    if let Some(algorithm) = Algorithm::from_id(params::text(params, "algorithm")) {
        return Ok(hash_text(algorithm, text));
    }
    let lines: Vec<String> = Algorithm::ALL
        .into_iter()
        .map(|a| format!("{}: {}", a.name(), hash_text(a, text)))
        .collect();
    Ok(lines.join("\n"))
}
