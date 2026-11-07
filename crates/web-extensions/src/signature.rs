use std::collections::BTreeSet;
use std::fmt;

use aws_lc_rs::digest::{SHA256, digest};
use aws_lc_rs::signature::{ECDSA_P256_SHA256_ASN1, RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use webpki::types::{CertificateDer, TrustAnchor, UnixTime};
use webpki::{EndEntityCert, KeyUsage, anchor_from_trusted_cert};

use crate::package::{Package, PackageError};

const MOZILLA_ROOT: &[u8] = include_bytes!("../data/mozilla-addons-root.der");
const COSE_SIGNATURE: &str = "META-INF/cose.sig";
const COSE_MANIFEST: &str = "META-INF/cose.manifest";
const CODE_SIGNING: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x03];
const COSE_SIGN_TAG: u64 = 98;
const COSE_ALGORITHM: i64 = 1;
const COSE_KID: i64 = 4;
const COSE_ES256: i64 = -7;
const COSE_PS256: i64 = -37;
const CN_OID: &[u8] = &[0x55, 0x04, 0x03];
const CBOR_MAX_DEPTH: usize = 8;
const CRX_MAGIC: &[u8; 4] = b"Cr24";
const CRX_VERSION: u32 = 3;
const CRX_CONTEXT: &[u8] = b"CRX3 SignedData\x00";
const CRX_ID_SIZE: usize = 16;
const CRX_MAX_HEADER: usize = 1 << 20;
const CRX_FIELD_RSA: u64 = 2;
const CRX_FIELD_ECDSA: u64 = 3;
const CRX_FIELD_SIGNED: u64 = 10_000;
const GOOGLE_PUBLISHER_KEY_HASH: [u8; 32] = [
    0x61, 0xf7, 0xf2, 0xa6, 0xbf, 0xcf, 0x74, 0xcd, 0x0b, 0xc1, 0xfe, 0x24, 0x97, 0xcc, 0x9b, 0x04,
    0x25, 0x4c, 0x65, 0x8f, 0x79, 0xf2, 0x14, 0x53, 0x92, 0x86, 0x7e, 0xa8, 0x36, 0x63, 0x67, 0xcf,
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Signed {
    Mozilla,
    ChromeWebStore,
    AuthorOnly,
    Unsigned,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureError {
    Malformed,
    Untrusted,
    WrongSignature,
    WrongIdentity,
    FilesChanged,
}

impl fmt::Display for SignatureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Malformed => "the extension's signature is damaged",
            Self::Untrusted => "the extension's signature does not come from a trusted store",
            Self::WrongSignature => "the extension's signature is not valid",
            Self::WrongIdentity => "the extension's signature is for another extension",
            Self::FilesChanged => "the extension's files do not match its signature",
        })
    }
}

impl std::error::Error for SignatureError {}

enum Cbor<'a> {
    Integer(i64),
    Bytes(&'a [u8]),
    Array(Vec<Cbor<'a>>),
    Map(Vec<(Cbor<'a>, Cbor<'a>)>),
    Tag(u64, Box<Cbor<'a>>),
    Null,
}

struct CborReader<'a> {
    bytes: &'a [u8],
}

impl<'a> CborReader<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], SignatureError> {
        let (taken, rest) = self
            .bytes
            .split_at_checked(count)
            .ok_or(SignatureError::Malformed)?;
        self.bytes = rest;
        Ok(taken)
    }

    fn argument(&mut self, info: u8) -> Result<u64, SignatureError> {
        let width = match info {
            0..=23 => return Ok(u64::from(info)),
            24 => 1,
            25 => 2,
            26 => 4,
            27 => 8,
            _ => return Err(SignatureError::Malformed),
        };
        Ok(self
            .take(width)?
            .iter()
            .fold(0, |value, byte| (value << 8) | u64::from(*byte)))
    }

    fn count(&mut self, info: u8) -> Result<usize, SignatureError> {
        let count = usize::try_from(self.argument(info)?).map_err(|_| SignatureError::Malformed)?;
        (count <= self.bytes.len())
            .then_some(count)
            .ok_or(SignatureError::Malformed)
    }

    fn value(&mut self, depth: usize) -> Result<Cbor<'a>, SignatureError> {
        if depth > CBOR_MAX_DEPTH {
            return Err(SignatureError::Malformed);
        }
        let first = *self.take(1)?.first().ok_or(SignatureError::Malformed)?;
        let info = first & 0x1f;
        match first >> 5 {
            0 => i64::try_from(self.argument(info)?)
                .map(Cbor::Integer)
                .map_err(|_| SignatureError::Malformed),
            1 => i64::try_from(self.argument(info)?)
                .map(|value| Cbor::Integer(-1 - value))
                .map_err(|_| SignatureError::Malformed),
            2 => {
                let count = self.count(info)?;
                self.take(count).map(Cbor::Bytes)
            }
            4 => {
                let count = self.count(info)?;
                (0..count)
                    .map(|_| self.value(depth + 1))
                    .collect::<Result<_, _>>()
                    .map(Cbor::Array)
            }
            5 => {
                let count = self.count(info)?;
                (0..count)
                    .map(|_| Ok((self.value(depth + 1)?, self.value(depth + 1)?)))
                    .collect::<Result<_, _>>()
                    .map(Cbor::Map)
            }
            6 => {
                let tag = self.argument(info)?;
                Ok(Cbor::Tag(tag, Box::new(self.value(depth + 1)?)))
            }
            7 if info == 22 => Ok(Cbor::Null),
            _ => Err(SignatureError::Malformed),
        }
    }
}

fn parse_cbor(bytes: &[u8]) -> Result<Cbor<'_>, SignatureError> {
    let mut reader = CborReader { bytes };
    let value = reader.value(0)?;
    reader
        .bytes
        .is_empty()
        .then_some(value)
        .ok_or(SignatureError::Malformed)
}

fn map_entry<'b, 'a>(map: &'b Cbor<'a>, key: i64) -> Option<&'b Cbor<'a>> {
    let Cbor::Map(entries) = map else {
        return None;
    };
    entries.iter().find_map(|(name, value)| match name {
        Cbor::Integer(name) if *name == key => Some(value),
        _ => None,
    })
}

fn cbor_head(output: &mut Vec<u8>, major: u8, length: usize) {
    let major = major << 5;
    match length {
        0..=23 => output.push(major | length as u8),
        24..=0xff => output.extend([major | 24, length as u8]),
        0x100..=0xffff => {
            output.push(major | 25);
            output.extend((length as u16).to_be_bytes());
        }
        _ => {
            output.push(major | 26);
            output.extend((length as u32).to_be_bytes());
        }
    }
}

fn cbor_bytes(output: &mut Vec<u8>, bytes: &[u8]) {
    cbor_head(output, 2, bytes.len());
    output.extend_from_slice(bytes);
}

fn cose_signed_data(body: &[u8], signer: &[u8], payload: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(payload.len() + body.len() + signer.len() + 32);
    cbor_head(&mut output, 4, 5);
    cbor_head(&mut output, 3, "Signature".len());
    output.extend_from_slice(b"Signature");
    cbor_bytes(&mut output, body);
    cbor_bytes(&mut output, signer);
    cbor_bytes(&mut output, &[]);
    cbor_bytes(&mut output, payload);
    output
}

fn der_element(bytes: &[u8]) -> Option<(u8, &[u8], &[u8])> {
    let (&tag, rest) = bytes.split_first()?;
    let (&first, rest) = rest.split_first()?;
    let (length, rest) = match first {
        0..=0x7f => (usize::from(first), rest),
        0x81..=0x84 => {
            let (raw, rest) = rest.split_at_checked(usize::from(first & 0x7f))?;
            let length = raw
                .iter()
                .fold(0usize, |value, byte| (value << 8) | usize::from(*byte));
            (length, rest)
        }
        _ => return None,
    };
    let (content, rest) = rest.split_at_checked(length)?;
    Some((tag, content, rest))
}

fn common_name(certificate: &[u8]) -> Option<String> {
    const SEQUENCE: u8 = 0x30;
    const SET: u8 = 0x31;
    const EXPLICIT_VERSION: u8 = 0xa0;
    let (_, certificate, _) = der_element(certificate)?;
    let (_, tbs, _) = der_element(certificate)?;
    let (tag, _, mut rest) = der_element(tbs)?;
    if tag == EXPLICIT_VERSION {
        (_, _, rest) = der_element(rest)?;
    }
    (_, _, rest) = der_element(rest)?;
    (_, _, rest) = der_element(rest)?;
    (_, _, rest) = der_element(rest)?;
    let (tag, mut subject, _) = der_element(rest)?;
    if tag != SEQUENCE {
        return None;
    }
    while !subject.is_empty() {
        let (tag, set, after) = der_element(subject)?;
        subject = after;
        if tag != SET {
            return None;
        }
        let (_, pair, _) = der_element(set)?;
        let (_, oid, value) = der_element(pair)?;
        if oid == CN_OID {
            let (_, name, _) = der_element(value)?;
            return std::str::from_utf8(name).ok().map(str::to_owned);
        }
    }
    None
}

fn base64(text: &str) -> Option<Vec<u8>> {
    let mut output = Vec::with_capacity(text.len() / 4 * 3);
    let mut buffer = 0u32;
    let mut bits = 0;
    for byte in text.bytes().filter(|byte| *byte != b'=') {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        buffer = (buffer << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((buffer >> bits) as u8);
        }
    }
    Some(output)
}

fn check_file_digests(package: &Package, manifest: &[u8]) -> Result<(), SignatureError> {
    let text = std::str::from_utf8(manifest).map_err(|_| SignatureError::Malformed)?;
    let unfolded = text.replace("\r\n", "\n").replace("\n ", "");
    let mut listed = BTreeSet::new();
    let mut name: Option<&str> = None;
    for line in unfolded.lines() {
        if let Some(value) = line.strip_prefix("Name: ") {
            name = Some(value);
        } else if let Some(value) = line.strip_prefix("SHA256-Digest: ") {
            let file = name.take().ok_or(SignatureError::Malformed)?;
            let contents = package
                .files
                .get(file)
                .ok_or(SignatureError::FilesChanged)?;
            let expected = base64(value).ok_or(SignatureError::Malformed)?;
            if digest(&SHA256, contents).as_ref() != expected.as_slice() {
                return Err(SignatureError::FilesChanged);
            }
            listed.insert(file);
        }
    }
    let all_listed = package
        .files
        .keys()
        .all(|file| file == COSE_SIGNATURE || file == COSE_MANIFEST || listed.contains(file.as_str()));
    if all_listed {
        Ok(())
    } else {
        Err(SignatureError::FilesChanged)
    }
}

struct Signer<'a> {
    algorithm: i64,
    certificate: &'a [u8],
    protected: &'a [u8],
    signature: &'a [u8],
}

fn signers<'b, 'a>(list: &'b Cbor<'a>) -> Result<Vec<Signer<'a>>, SignatureError> {
    let Cbor::Array(list) = list else {
        return Err(SignatureError::Malformed);
    };
    list.iter()
        .map(|entry| {
            let Cbor::Array(parts) = entry else {
                return Err(SignatureError::Malformed);
            };
            let [Cbor::Bytes(protected), _, Cbor::Bytes(signature)] = parts.as_slice() else {
                return Err(SignatureError::Malformed);
            };
            let header = parse_cbor(protected)?;
            let (Some(Cbor::Integer(algorithm)), Some(Cbor::Bytes(certificate))) =
                (map_entry(&header, COSE_ALGORITHM), map_entry(&header, COSE_KID))
            else {
                return Err(SignatureError::Malformed);
            };
            Ok(Signer {
                algorithm: *algorithm,
                certificate,
                protected,
                signature,
            })
        })
        .collect()
}

fn verify_signer(
    signer: &Signer<'_>,
    intermediates: &[CertificateDer<'_>],
    anchors: &[TrustAnchor<'_>],
    signed: &[u8],
    id: &str,
    now: UnixTime,
) -> Result<(), SignatureError> {
    let algorithm = match signer.algorithm {
        COSE_ES256 => webpki::aws_lc_rs::ECDSA_P256_SHA256,
        COSE_PS256 => webpki::aws_lc_rs::RSA_PSS_2048_8192_SHA256_LEGACY_KEY,
        _ => return Err(SignatureError::WrongSignature),
    };
    let der = CertificateDer::from(signer.certificate);
    let certificate = EndEntityCert::try_from(&der).map_err(|_| SignatureError::Malformed)?;
    certificate
        .verify_for_usage(
            webpki::ALL_VERIFICATION_ALGS,
            anchors,
            intermediates,
            now,
            KeyUsage::required(CODE_SIGNING),
            None,
            None,
        )
        .map_err(|_| SignatureError::Untrusted)?;
    certificate
        .verify_signature(algorithm, signed, signer.signature)
        .map_err(|_| SignatureError::WrongSignature)?;
    if common_name(signer.certificate).as_deref() != Some(id) {
        return Err(SignatureError::WrongIdentity);
    }
    Ok(())
}

pub(crate) fn verify_xpi(package: &Package, id: Option<&str>) -> Result<Signed, SignatureError> {
    let Some(signature) = package.files.get(COSE_SIGNATURE) else {
        return Ok(Signed::Unsigned);
    };
    let payload = package
        .files
        .get(COSE_MANIFEST)
        .ok_or(SignatureError::Malformed)?;
    let id = id.ok_or(SignatureError::WrongIdentity)?;
    let root = CertificateDer::from(MOZILLA_ROOT);
    let anchor = anchor_from_trusted_cert(&root).map_err(|_| SignatureError::Malformed)?;
    let parsed = parse_cbor(signature)?;
    let Cbor::Tag(COSE_SIGN_TAG, body) = &parsed else {
        return Err(SignatureError::Malformed);
    };
    let Cbor::Array(parts) = body.as_ref() else {
        return Err(SignatureError::Malformed);
    };
    let [Cbor::Bytes(protected), _, Cbor::Null, list] = parts.as_slice() else {
        return Err(SignatureError::Malformed);
    };
    let header = parse_cbor(protected)?;
    let intermediates: Vec<CertificateDer<'_>> = match map_entry(&header, COSE_KID) {
        Some(Cbor::Array(certificates)) => certificates
            .iter()
            .filter_map(|certificate| match certificate {
                Cbor::Bytes(der) => Some(CertificateDer::from(*der)),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    };
    let now = UnixTime::now();
    let mut last = SignatureError::WrongSignature;
    let mut accepted = false;
    for signer in signers(list)? {
        let signed = cose_signed_data(protected, signer.protected, payload);
        let result = verify_signer(
            &signer,
            &intermediates,
            std::slice::from_ref(&anchor),
            &signed,
            id,
            now,
        );
        match result {
            Ok(()) => accepted = true,
            Err(error) => last = error,
        }
    }
    if !accepted {
        return Err(last);
    }
    check_file_digests(package, payload)?;
    Ok(Signed::Mozilla)
}

fn varint(bytes: &mut &[u8]) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let (&byte, rest) = bytes.split_first()?;
        *bytes = rest;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn proto_fields(mut bytes: &[u8]) -> Option<Vec<(u64, &[u8])>> {
    const LENGTH_DELIMITED: u64 = 2;
    let mut fields = Vec::new();
    while !bytes.is_empty() {
        let key = varint(&mut bytes)?;
        if key & 7 != LENGTH_DELIMITED {
            return None;
        }
        let length = usize::try_from(varint(&mut bytes)?).ok()?;
        let (value, rest) = bytes.split_at_checked(length)?;
        bytes = rest;
        fields.push((key >> 3, value));
    }
    Some(fields)
}

pub struct Crx<'a> {
    pub id: String,
    pub signed: Signed,
    pub zip: &'a [u8],
}

pub(crate) fn is_crx(bytes: &[u8]) -> bool {
    bytes.starts_with(CRX_MAGIC)
}

fn extension_id(hash: &[u8]) -> String {
    hash.iter()
        .flat_map(|byte| [byte >> 4, byte & 0xf])
        .map(|digit| char::from(b'a' + digit))
        .collect()
}

pub(crate) fn verify_crx(bytes: &[u8]) -> Result<Crx<'_>, PackageError2> {
    let malformed = PackageError2::Signature(SignatureError::Malformed);
    let rest = bytes.strip_prefix(CRX_MAGIC).ok_or(malformed)?;
    let (version, rest) = rest.split_at_checked(4).ok_or(malformed)?;
    let version = u32::from_le_bytes(version.try_into().map_err(|_| malformed)?);
    if version != CRX_VERSION {
        return Err(PackageError2::Package(PackageError::Unsupported));
    }
    let (size, rest) = rest.split_at_checked(4).ok_or(malformed)?;
    let size = u32::from_le_bytes(size.try_into().map_err(|_| malformed)?) as usize;
    if size > CRX_MAX_HEADER {
        return Err(malformed);
    }
    let (header, zip) = rest.split_at_checked(size).ok_or(malformed)?;
    let fields = proto_fields(header).ok_or(malformed)?;
    let signed = fields
        .iter()
        .find(|(field, _)| *field == CRX_FIELD_SIGNED)
        .map(|(_, value)| *value)
        .ok_or(malformed)?;
    let hash = proto_fields(signed)
        .and_then(|fields| {
            fields
                .iter()
                .find(|(field, _)| *field == 1)
                .map(|(_, value)| *value)
        })
        .filter(|hash| hash.len() == CRX_ID_SIZE)
        .ok_or(malformed)?;
    let mut message = Vec::with_capacity(CRX_CONTEXT.len() + 4 + signed.len() + zip.len());
    message.extend_from_slice(CRX_CONTEXT);
    message.extend_from_slice(&(signed.len() as u32).to_le_bytes());
    message.extend_from_slice(signed);
    message.extend_from_slice(zip);
    let mut author = false;
    let mut publisher = false;
    for (field, proof) in &fields {
        if *field != CRX_FIELD_RSA && *field != CRX_FIELD_ECDSA {
            continue;
        }
        let parts = proto_fields(proof).ok_or(malformed)?;
        let part = |number: u64| {
            parts
                .iter()
                .find(|(field, _)| *field == number)
                .map(|(_, value)| *value)
        };
        let (Some(key), Some(signature)) = (part(1), part(2)) else {
            return Err(malformed);
        };
        let verified = match *field {
            CRX_FIELD_RSA => {
                UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, key).verify(&message, signature)
            }
            _ => UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, key).verify(&message, signature),
        };
        verified.map_err(|_| PackageError2::Signature(SignatureError::WrongSignature))?;
        let key_hash = digest(&SHA256, key);
        author |= key_hash.as_ref().get(..CRX_ID_SIZE) == Some(hash);
        publisher |= key_hash.as_ref() == GOOGLE_PUBLISHER_KEY_HASH;
    }
    if !author {
        return Err(PackageError2::Signature(SignatureError::WrongIdentity));
    }
    Ok(Crx {
        id: extension_id(hash),
        signed: if publisher {
            Signed::ChromeWebStore
        } else {
            Signed::AuthorOnly
        },
        zip,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PackageError2 {
    Package(PackageError),
    Signature(SignatureError),
}
