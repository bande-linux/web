use encoding_rs::Encoding;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContentType {
    pub essence: String,
    pub charset: Option<&'static Encoding>,
}

impl ContentType {
    pub fn parse(value: &[u8]) -> Option<Self> {
        let value = trim(value);
        let slash = value.iter().position(|&byte| byte == b'/')?;
        let kind = &value[..slash];
        let rest = &value[slash + 1..];
        let end = rest
            .iter()
            .position(|&byte| byte == b';')
            .unwrap_or(rest.len());
        let subtype = trim_end(&rest[..end]);
        if !is_token(kind) || !is_token(subtype) {
            return None;
        }
        let essence = format!(
            "{}/{}",
            String::from_utf8_lossy(kind).to_ascii_lowercase(),
            String::from_utf8_lossy(subtype).to_ascii_lowercase()
        );
        Some(Self {
            essence,
            charset: charset(&rest[end..]),
        })
    }

    pub fn is_html(&self) -> bool {
        matches!(self.essence.as_str(), "text/html" | "application/xhtml+xml")
    }

    pub fn is_viewable(&self) -> bool {
        is_viewable(&self.essence)
    }
}

pub fn is_viewable(essence: &str) -> bool {
    matches!(
        essence,
        "application/json"
            | "application/xml"
            | "image/avif"
            | "image/gif"
            | "image/jpeg"
            | "image/jxl"
            | "image/png"
            | "image/webp"
    ) || (essence.starts_with("text/") && essence != "text/html")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Disposition {
    pub attachment: bool,
    pub name: Option<String>,
}

pub(crate) fn percent_decoded(text: &str) -> Vec<u8> {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let hex = (byte == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match hex {
            Some(value) => {
                out.push(value);
                index += 3;
            }
            None => {
                out.push(byte);
                index += 1;
            }
        }
    }
    out
}

fn extended_value(value: &str) -> Option<String> {
    let mut parts = value.splitn(3, '\'');
    let charset = parts.next()?;
    parts.next()?;
    let encoded = parts.next()?;
    let encoding = Encoding::for_label(charset.as_bytes())?;
    let bytes = percent_decoded(encoded);
    Some(encoding.decode(&bytes).0.into_owned())
}

impl Disposition {
    pub fn parse(value: &[u8]) -> Self {
        let value = String::from_utf8_lossy(value);
        let mut pieces = split_parameters(&value).into_iter();
        let attachment = pieces
            .next()
            .is_some_and(|kind| kind.trim().eq_ignore_ascii_case("attachment"));
        let (mut plain, mut extended) = (None, None);
        for piece in pieces {
            let Some((name, value)) = piece.split_once('=') else {
                continue;
            };
            let value = value.trim();
            match name.trim().to_ascii_lowercase().as_str() {
                "filename" => plain = Some(unquoted(value)),
                "filename*" => extended = extended_value(value),
                _ => {}
            }
        }
        Self {
            attachment,
            name: extended.or(plain).filter(|name| !name.is_empty()),
        }
    }
}

fn split_parameters(value: &str) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut start, mut quoted, mut escaped) = (0, false, false);
    for (index, character) in value.char_indices() {
        match character {
            _ if escaped => escaped = false,
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ';' if !quoted => {
                pieces.push(&value[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    pieces.push(&value[start..]);
    pieces
}

fn unquoted(value: &str) -> String {
    let Some(inner) = value.strip_prefix('"') else {
        return value.to_owned();
    };
    let mut text = String::with_capacity(inner.len());
    let mut characters = inner.chars();
    while let Some(character) = characters.next() {
        match character {
            '"' => break,
            '\\' => text.extend(characters.next()),
            character => text.push(character),
        }
    }
    text
}

fn charset(mut parameters: &[u8]) -> Option<&'static Encoding> {
    while let Some((&b';', rest)) = parameters.split_first() {
        let rest = trim_start(rest);
        let name_end = rest
            .iter()
            .position(|&byte| byte == b';' || byte == b'=')
            .unwrap_or(rest.len());
        let name = &rest[..name_end];
        let rest = &rest[name_end..];
        let Some((&b'=', rest)) = rest.split_first() else {
            parameters = rest;
            continue;
        };
        let (value, rest) = match rest.split_first() {
            Some((&b'"', quoted)) => quoted_string(quoted),
            _ => {
                let end = rest
                    .iter()
                    .position(|&byte| byte == b';')
                    .unwrap_or(rest.len());
                (trim_end(&rest[..end]).to_vec(), &rest[end..])
            }
        };
        if name.eq_ignore_ascii_case(b"charset") && !value.is_empty() {
            return Encoding::for_label(&value);
        }
        parameters = rest;
    }
    None
}

fn quoted_string(mut input: &[u8]) -> (Vec<u8>, &[u8]) {
    let mut value = Vec::new();
    while let Some((&byte, rest)) = input.split_first() {
        input = rest;
        match byte {
            b'"' => break,
            b'\\' => {
                if let Some((&escaped, rest)) = input.split_first() {
                    value.push(escaped);
                    input = rest;
                }
            }
            byte => value.push(byte),
        }
    }
    let end = input
        .iter()
        .position(|&byte| byte == b';')
        .unwrap_or(input.len());
    (value, &input[end..])
}

fn is_token(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes
            .iter()
            .all(|&byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
}

fn is_http_whitespace(byte: &u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r')
}

fn trim_start(bytes: &[u8]) -> &[u8] {
    let start = bytes
        .iter()
        .position(|byte| !is_http_whitespace(byte))
        .unwrap_or(bytes.len());
    &bytes[start..]
}

fn trim_end(bytes: &[u8]) -> &[u8] {
    let end = bytes
        .iter()
        .rposition(|byte| !is_http_whitespace(byte))
        .map_or(0, |end| end + 1);
    &bytes[..end]
}

fn trim(bytes: &[u8]) -> &[u8] {
    trim_end(trim_start(bytes))
}
