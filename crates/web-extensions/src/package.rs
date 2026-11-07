use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

use flate2::Crc;
use flate2::read::DeflateDecoder;

const END_RECORD: &[u8; 4] = b"PK\x05\x06";
const CENTRAL_RECORD: &[u8; 4] = b"PK\x01\x02";
const LOCAL_RECORD: &[u8; 4] = b"PK\x03\x04";
const END_SIZE: usize = 22;
const CENTRAL_SIZE: usize = 46;
const LOCAL_SIZE: usize = 30;
const MAX_COMMENT: usize = 0xffff;
pub const MAX_ARCHIVE: usize = 256 << 20;
const MAX_FILE: u64 = 256 << 20;
const MAX_TOTAL: u64 = 1 << 30;
const MAX_FILES: usize = 65_535;
const STORED: u16 = 0;
const DEFLATED: u16 = 8;
const ENCRYPTED_FLAG: u16 = 1;
const UNIX_CREATOR: u8 = 3;
const FILE_TYPE_MASK: u32 = 0o170_000;
const FILE_TYPE_REGULAR: u32 = 0o100_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackageError {
    TooLarge,
    NotArchive,
    Corrupt,
    UnsafePath,
    Unsupported,
}

impl fmt::Display for PackageError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooLarge => "the extension is too large",
            Self::NotArchive => "the file is not an extension package",
            Self::Corrupt => "the extension package is damaged",
            Self::UnsafePath => "the extension package has a file outside itself",
            Self::Unsupported => "the extension package uses an unsupported format",
        })
    }
}

impl std::error::Error for PackageError {}

#[derive(Debug, Default)]
pub struct Package {
    pub files: BTreeMap<String, Vec<u8>>,
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16, PackageError> {
    let end = offset.checked_add(2).ok_or(PackageError::Corrupt)?;
    let slice = bytes.get(offset..end).ok_or(PackageError::Corrupt)?;
    Ok(u16::from_le_bytes(
        slice.try_into().map_err(|_| PackageError::Corrupt)?,
    ))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32, PackageError> {
    let end = offset.checked_add(4).ok_or(PackageError::Corrupt)?;
    let slice = bytes.get(offset..end).ok_or(PackageError::Corrupt)?;
    Ok(u32::from_le_bytes(
        slice.try_into().map_err(|_| PackageError::Corrupt)?,
    ))
}

fn safe_path(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains('\0')
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

fn end_of_central(bytes: &[u8]) -> Result<usize, PackageError> {
    let earliest = bytes.len().saturating_sub(END_SIZE + MAX_COMMENT);
    let latest = bytes
        .len()
        .checked_sub(END_SIZE)
        .ok_or(PackageError::NotArchive)?;
    (earliest..=latest)
        .rev()
        .find(|&offset| {
            bytes.get(offset..offset + 4) == Some(END_RECORD.as_slice())
                && u16_at(bytes, offset + 20)
                    .is_ok_and(|comment| offset + END_SIZE + usize::from(comment) == bytes.len())
        })
        .ok_or(PackageError::NotArchive)
}

struct Entry<'a> {
    name: &'a str,
    method: u16,
    crc: u32,
    compressed: usize,
    size: u64,
    local: usize,
}

fn inflate(entry: &Entry<'_>, data: &[u8]) -> Result<Vec<u8>, PackageError> {
    let capacity = usize::try_from(entry.size).map_err(|_| PackageError::TooLarge)?;
    let mut output = Vec::with_capacity(capacity.min(1 << 20));
    match entry.method {
        STORED => output.extend_from_slice(data),
        DEFLATED => {
            DeflateDecoder::new(data)
                .take(entry.size.saturating_add(1))
                .read_to_end(&mut output)
                .map_err(|_| PackageError::Corrupt)?;
        }
        _ => return Err(PackageError::Unsupported),
    }
    if output.len() as u64 != entry.size {
        return Err(PackageError::Corrupt);
    }
    let mut crc = Crc::new();
    crc.update(&output);
    if crc.sum() != entry.crc {
        return Err(PackageError::Corrupt);
    }
    Ok(output)
}

fn data_of<'a>(bytes: &'a [u8], entry: &Entry<'_>) -> Result<&'a [u8], PackageError> {
    if bytes.get(entry.local..entry.local.saturating_add(4)) != Some(LOCAL_RECORD.as_slice()) {
        return Err(PackageError::Corrupt);
    }
    let name = usize::from(u16_at(bytes, entry.local + 26)?);
    let extra = usize::from(u16_at(bytes, entry.local + 28)?);
    let start = entry
        .local
        .checked_add(LOCAL_SIZE + name + extra)
        .ok_or(PackageError::Corrupt)?;
    let end = start
        .checked_add(entry.compressed)
        .ok_or(PackageError::Corrupt)?;
    bytes.get(start..end).ok_or(PackageError::Corrupt)
}

pub fn read_zip(bytes: &[u8]) -> Result<Package, PackageError> {
    if bytes.len() > MAX_ARCHIVE {
        return Err(PackageError::TooLarge);
    }
    let end = end_of_central(bytes)?;
    let count = usize::from(u16_at(bytes, end + 10)?);
    let mut offset = u32_at(bytes, end + 16)? as usize;
    if count > MAX_FILES || u32_at(bytes, end + 16)? == u32::MAX {
        return Err(PackageError::Unsupported);
    }
    let mut package = Package::default();
    let mut total: u64 = 0;
    for _ in 0..count {
        if bytes.get(offset..offset.saturating_add(4)) != Some(CENTRAL_RECORD.as_slice()) {
            return Err(PackageError::Corrupt);
        }
        let name_size = usize::from(u16_at(bytes, offset + 28)?);
        let extra_size = usize::from(u16_at(bytes, offset + 30)?);
        let comment_size = usize::from(u16_at(bytes, offset + 32)?);
        let name_start = offset + CENTRAL_SIZE;
        let raw_name = bytes
            .get(name_start..name_start + name_size)
            .ok_or(PackageError::Corrupt)?;
        let name = std::str::from_utf8(raw_name).map_err(|_| PackageError::UnsafePath)?;
        if u16_at(bytes, offset + 8)? & ENCRYPTED_FLAG != 0 {
            return Err(PackageError::Unsupported);
        }
        let creator = bytes.get(offset + 5).copied().ok_or(PackageError::Corrupt)?;
        let mode = u32_at(bytes, offset + 38)? >> 16;
        let regular = creator != UNIX_CREATOR
            || mode & FILE_TYPE_MASK == 0
            || mode & FILE_TYPE_MASK == FILE_TYPE_REGULAR;
        let entry = Entry {
            name,
            method: u16_at(bytes, offset + 10)?,
            crc: u32_at(bytes, offset + 16)?,
            compressed: u32_at(bytes, offset + 20)? as usize,
            size: u64::from(u32_at(bytes, offset + 24)?),
            local: u32_at(bytes, offset + 42)? as usize,
        };
        offset = name_start + name_size + extra_size + comment_size;
        let directory = name.strip_suffix('/');
        if !safe_path(directory.unwrap_or(name)) {
            return Err(PackageError::UnsafePath);
        }
        if directory.is_some() {
            continue;
        }
        if !regular {
            return Err(PackageError::UnsafePath);
        }
        total = total.saturating_add(entry.size);
        if entry.size > MAX_FILE || total > MAX_TOTAL {
            return Err(PackageError::TooLarge);
        }
        let contents = inflate(&entry, data_of(bytes, &entry)?)?;
        if package.files.insert(entry.name.to_owned(), contents).is_some() {
            return Err(PackageError::Corrupt);
        }
    }
    Ok(package)
}
