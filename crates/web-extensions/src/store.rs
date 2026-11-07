use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::Crc;

const HEADER: u64 = 8;
const MAX_BATCH: u64 = 1 << 30;
const COMPACT_ABOVE: u64 = 1 << 20;
const SET: u8 = 1;
const REMOVE: u8 = 2;
const CLEAR: u8 = 3;

pub const DEFAULT_QUOTA: u64 = 10 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    Set(String, Vec<u8>),
    Remove(String),
    Clear,
}

pub struct Store {
    path: PathBuf,
    file: File,
    index: BTreeMap<String, (u64, u64)>,
    end: u64,
    quota: Option<u64>,
}

fn checksum(bytes: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(bytes);
    crc.sum()
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (taken, rest) = bytes.split_at_checked(count)?;
    *bytes = rest;
    Some(taken)
}

fn take_u32(bytes: &mut &[u8]) -> Option<usize> {
    let raw = take(bytes, 4)?;
    Some(u32::from_le_bytes(raw.try_into().ok()?) as usize)
}

fn encode(changes: &[Change]) -> Vec<u8> {
    let mut batch = Vec::new();
    for change in changes {
        match change {
            Change::Set(key, value) => {
                batch.push(SET);
                batch.extend((key.len() as u32).to_le_bytes());
                batch.extend((value.len() as u32).to_le_bytes());
                batch.extend_from_slice(key.as_bytes());
                batch.extend_from_slice(value);
            }
            Change::Remove(key) => {
                batch.push(REMOVE);
                batch.extend((key.len() as u32).to_le_bytes());
                batch.extend_from_slice(key.as_bytes());
            }
            Change::Clear => batch.push(CLEAR),
        }
    }
    let mut record = Vec::with_capacity(batch.len() + HEADER as usize);
    record.extend((batch.len() as u32).to_le_bytes());
    record.extend(checksum(&batch).to_le_bytes());
    record.extend(batch);
    record
}

fn apply(
    index: &mut BTreeMap<String, (u64, u64)>,
    mut batch: &[u8],
    position: u64,
) -> Option<()> {
    let start = batch.len();
    while let Some((&operation, rest)) = batch.split_first() {
        batch = rest;
        match operation {
            SET => {
                let key_size = take_u32(&mut batch)?;
                let value_size = take_u32(&mut batch)?;
                let key = std::str::from_utf8(take(&mut batch, key_size)?).ok()?;
                let offset = position + (start - batch.len()) as u64;
                take(&mut batch, value_size)?;
                index.insert(key.to_owned(), (offset, value_size as u64));
            }
            REMOVE => {
                let key_size = take_u32(&mut batch)?;
                let key = std::str::from_utf8(take(&mut batch, key_size)?).ok()?;
                index.remove(key);
            }
            CLEAR => index.clear(),
            _ => return None,
        }
    }
    Some(())
}

fn replay(bytes: &[u8]) -> (BTreeMap<String, (u64, u64)>, u64) {
    let mut index = BTreeMap::new();
    let mut position = 0u64;
    let mut rest = bytes;
    loop {
        let mut cursor = rest;
        let parsed = (|| {
            let size = take_u32(&mut cursor)?;
            let expected = take_u32(&mut cursor)? as u32;
            let batch = take(&mut cursor, size)?;
            (checksum(batch) == expected).then_some(batch)
        })();
        let Some(batch) = parsed else {
            return (index, position);
        };
        let mut next = index.clone();
        if apply(&mut next, batch, position + HEADER).is_none() {
            return (index, position);
        }
        index = next;
        position += HEADER + batch.len() as u64;
        rest = cursor;
    }
}

impl Store {
    pub fn open(path: &Path, quota: Option<u64>) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)?;
        let (index, end) = replay(&bytes);
        if end < bytes.len() as u64 {
            file.set_len(end)?;
        }
        Ok(Self {
            path: path.to_owned(),
            file,
            index,
            end,
            quota,
        })
    }

    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.index.keys().map(String::as_str)
    }

    pub fn get(&mut self, key: &str) -> io::Result<Option<Vec<u8>>> {
        let Some(&(offset, size)) = self.index.get(key) else {
            return Ok(None);
        };
        let mut value = vec![0; size as usize];
        self.file.seek(SeekFrom::Start(offset))?;
        self.file.read_exact(&mut value)?;
        Ok(Some(value))
    }

    pub fn bytes_in_use(&self) -> u64 {
        self.index
            .iter()
            .map(|(key, (_, size))| key.len() as u64 + size)
            .sum()
    }

    pub fn change(&mut self, changes: &[Change]) -> io::Result<()> {
        let record = encode(changes);
        if record.len() as u64 > MAX_BATCH {
            return Err(io::ErrorKind::FileTooLarge.into());
        }
        let mut next = self.index.clone();
        let batch = record.get(HEADER as usize..).unwrap_or_default();
        if apply(&mut next, batch, self.end + HEADER).is_none() {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        if let Some(quota) = self.quota {
            let used: u64 = next
                .iter()
                .map(|(key, (_, size))| key.len() as u64 + size)
                .sum();
            if used > quota {
                return Err(io::ErrorKind::QuotaExceeded.into());
            }
        }
        self.file.seek(SeekFrom::Start(self.end))?;
        self.file.write_all(&record)?;
        self.file.sync_data()?;
        self.index = next;
        self.end += record.len() as u64;
        let live = self.bytes_in_use();
        if self.end > COMPACT_ABOVE && self.end / 2 > live {
            self.compact()?;
        }
        Ok(())
    }

    fn compact(&mut self) -> io::Result<()> {
        let keys: Vec<String> = self.index.keys().cloned().collect();
        let mut changes = Vec::with_capacity(keys.len());
        for key in keys {
            let value = self.get(&key)?.unwrap_or_default();
            changes.push(Change::Set(key, value));
        }
        let record = encode(&changes);
        let temporary = self.path.with_extension("compacting");
        let mut file = File::create(&temporary)?;
        file.write_all(&record)?;
        file.sync_all()?;
        fs::rename(&temporary, &self.path)?;
        self.file = OpenOptions::new().read(true).write(true).open(&self.path)?;
        let (index, end) = replay(&record);
        self.index = index;
        self.end = end;
        Ok(())
    }
}
