// SPDX-License-Identifier: Apache-2.0
//! Original journal bodies are preserved. Layout 3 extends the authenticated
//! frame header with a publication digest; neither that digest nor the payload
//! can change without invalidating the existing chain/commit marker checks.
use crate::{contract::MAX_COMMIT_BYTES, Error, Result};
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
};
use vcp_protocol::digest_bytes;

pub(crate) const LEGACY_MAGIC: &[u8; 8] = b"VCPJ0001";
pub(crate) const CURRENT_MAGIC: &[u8; 8] = b"VCPJ0003";
pub(crate) const COMMITTED: &[u8; 8] = b"VCPCMIT1";
pub(crate) const LEGACY_HEADER: usize = 80;
pub(crate) const CURRENT_HEADER: usize = LEGACY_HEADER + 64;
const TRAILER: usize = 72;

pub(crate) enum ReadFrame {
    End,
    Incomplete,
    Complete(Frame),
}
pub(crate) struct Frame {
    pub(crate) payload: Vec<u8>,
    pub(crate) publication: Option<String>,
    pub(crate) chain: String,
    pub(crate) end: u64,
}
fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
pub(crate) fn header(payload: &[u8], prior: &str, publication: Option<&str>) -> Result<Vec<u8>> {
    if payload.is_empty() || payload.len() > MAX_COMMIT_BYTES {
        return Err(Error::Limit("commit bytes"));
    }
    if !valid_digest(prior) || publication.is_some_and(|digest| !valid_digest(digest)) {
        return Err(Error::Corruption("journal frame digest"));
    }
    let mut header = Vec::with_capacity(if publication.is_some() {
        CURRENT_HEADER
    } else {
        LEGACY_HEADER
    });
    header.extend_from_slice(if publication.is_some() {
        CURRENT_MAGIC
    } else {
        LEGACY_MAGIC
    });
    header.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    header.extend_from_slice(&(!(payload.len() as u32)).to_le_bytes());
    header.extend_from_slice(prior.as_bytes());
    if let Some(publication) = publication {
        header.extend_from_slice(publication.as_bytes());
    }
    Ok(header)
}
pub(crate) fn read(file: &mut File, offset: u64, prior: &str) -> Result<ReadFrame> {
    let remaining = file
        .metadata()?
        .len()
        .checked_sub(offset)
        .ok_or(Error::Corruption("journal shrank"))?;
    if remaining == 0 {
        return Ok(ReadFrame::End);
    }
    if remaining < LEGACY_HEADER as u64 {
        return Ok(ReadFrame::Incomplete);
    }
    file.seek(SeekFrom::Start(offset))?;
    let mut header = vec![0; LEGACY_HEADER];
    file.read_exact(&mut header)?;
    let current = match &header[..8] {
        magic if magic == LEGACY_MAGIC => false,
        magic if magic == CURRENT_MAGIC => true,
        _ => return Err(Error::Corruption("journal frame version")),
    };
    let length = u32::from_le_bytes(
        header[8..12]
            .try_into()
            .map_err(|_| Error::Corruption("journal frame length"))?,
    ) as usize;
    let inverse = u32::from_le_bytes(
        header[12..16]
            .try_into()
            .map_err(|_| Error::Corruption("journal frame length"))?,
    );
    if length == 0
        || length > MAX_COMMIT_BYTES
        || inverse != !(length as u32)
        || header[16..] != *prior.as_bytes()
    {
        return Err(Error::Corruption("journal header or chain"));
    }
    if current {
        if remaining < CURRENT_HEADER as u64 {
            return Ok(ReadFrame::Incomplete);
        }
        header.resize(CURRENT_HEADER, 0);
        file.read_exact(&mut header[LEGACY_HEADER..])?;
    }
    let publication = if current {
        let digest = std::str::from_utf8(&header[LEGACY_HEADER..])
            .map_err(|_| Error::Corruption("journal publication digest"))?;
        if !valid_digest(digest) {
            return Err(Error::Corruption("journal publication digest"));
        }
        Some(digest.to_owned())
    } else {
        None
    };
    let extent = header
        .len()
        .checked_add(length)
        .and_then(|length| length.checked_add(TRAILER))
        .ok_or(Error::Corruption("journal extent"))?;
    if remaining < extent as u64 {
        return Ok(ReadFrame::Incomplete);
    }
    let mut payload = vec![0; length];
    file.read_exact(&mut payload)?;
    let mut trailer = [0; TRAILER];
    file.read_exact(&mut trailer)?;
    let chain = digest_bytes(&[header.as_slice(), payload.as_slice()].concat());
    if trailer[..64] != *chain.as_bytes() || &trailer[64..] != COMMITTED {
        return Err(Error::Corruption("committed journal bytes"));
    }
    Ok(ReadFrame::Complete(Frame {
        payload,
        publication,
        chain,
        end: offset
            .checked_add(extent as u64)
            .ok_or(Error::Corruption("journal extent"))?,
    }))
}
