use url::Url;

#[derive(Debug, Clone, PartialEq, Eq)]
enum Hosts {
    Any,
    Exact(String),
    WithSubdomains(String),
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchPattern {
    schemes: Vec<String>,
    hosts: Hosts,
    path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BadPattern;

const WILDCARD_SCHEMES: [&str; 4] = ["http", "https", "ws", "wss"];
const ALL_URLS_SCHEMES: [&str; 6] = ["http", "https", "ws", "wss", "ftp", "file"];
const NAMED_SCHEMES: [&str; 7] = ["http", "https", "ws", "wss", "ftp", "file", "data"];

impl MatchPattern {
    pub fn parse(text: &str) -> Result<Self, BadPattern> {
        if text == "<all_urls>" {
            return Ok(Self {
                schemes: ALL_URLS_SCHEMES.iter().map(|s| (*s).to_owned()).collect(),
                hosts: Hosts::Any,
                path: "/*".to_owned(),
            });
        }
        let (scheme, rest) = text.split_once("://").ok_or(BadPattern)?;
        let schemes = match scheme {
            "*" => WILDCARD_SCHEMES.iter().map(|s| (*s).to_owned()).collect(),
            name if NAMED_SCHEMES.contains(&name) => vec![name.to_owned()],
            _ => return Err(BadPattern),
        };
        let slash = rest.find('/').ok_or(BadPattern)?;
        let (host, path) = rest.split_at(slash);
        let hosts = match (scheme, host) {
            ("file", "") | ("file", "*") => Hosts::Local,
            (_, "") => return Err(BadPattern),
            (_, "*") => Hosts::Any,
            (_, host) => match host.strip_prefix("*.") {
                Some(domain) if !domain.is_empty() && !domain.contains('*') => {
                    Hosts::WithSubdomains(domain.to_ascii_lowercase())
                }
                None if !host.contains('*') => Hosts::Exact(host.to_ascii_lowercase()),
                _ => return Err(BadPattern),
            },
        };
        Ok(Self {
            schemes,
            hosts,
            path: path.to_owned(),
        })
    }

    pub fn matches(&self, url: &Url) -> bool {
        if !self.matches_origin(url) {
            return false;
        }
        let path = match url.query() {
            Some(query) => format!("{}?{query}", url.path()),
            None => url.path().to_owned(),
        };
        glob(&self.path, &path)
    }

    pub fn matches_origin(&self, url: &Url) -> bool {
        if !self.schemes.iter().any(|scheme| scheme == url.scheme()) {
            return false;
        }
        match &self.hosts {
            Hosts::Any => true,
            Hosts::Local => url.host().is_none(),
            Hosts::Exact(host) => url.host_str() == Some(host),
            Hosts::WithSubdomains(domain) => url.host_str().is_some_and(|host| {
                host == domain
                    || host
                        .strip_suffix(domain.as_str())
                        .is_some_and(|prefix| prefix.ends_with('.'))
            }),
        }
    }

    pub fn is_every_site(&self) -> bool {
        self.hosts == Hosts::Any
    }
}

fn glob(pattern: &str, text: &str) -> bool {
    let mut pieces = pattern.split('*');
    let Some(first) = pieces.next() else {
        return text.is_empty();
    };
    let Some(mut rest) = text.strip_prefix(first) else {
        return false;
    };
    let pieces: Vec<&str> = pieces.collect();
    let Some((last, middle)) = pieces.split_last() else {
        return rest.is_empty();
    };
    for piece in middle {
        let Some(found) = rest.find(piece) else {
            return false;
        };
        rest = rest.get(found + piece.len()..).unwrap_or_default();
    }
    rest.ends_with(last)
}

pub fn any_match(patterns: &[MatchPattern], url: &Url) -> bool {
    patterns.iter().any(|pattern| pattern.matches(url))
}

pub fn compile(patterns: &[String]) -> Vec<MatchPattern> {
    patterns
        .iter()
        .filter_map(|text| MatchPattern::parse(text).ok())
        .collect()
}
