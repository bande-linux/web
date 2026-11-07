use std::collections::BTreeMap;
use std::fmt;

use serde_json::{Map, Value};

use crate::pattern::MatchPattern;

const API_PERMISSIONS: &[&str] = &[
    "activeTab",
    "alarms",
    "bookmarks",
    "browserSettings",
    "browsingData",
    "clipboardRead",
    "clipboardWrite",
    "contextMenus",
    "cookies",
    "declarativeNetRequest",
    "declarativeNetRequestFeedback",
    "declarativeNetRequestWithHostAccess",
    "dns",
    "downloads",
    "downloads.open",
    "find",
    "geolocation",
    "history",
    "idle",
    "management",
    "menus",
    "menus.overrideContext",
    "nativeMessaging",
    "notifications",
    "offscreen",
    "pageCapture",
    "privacy",
    "proxy",
    "scripting",
    "search",
    "sessions",
    "storage",
    "tabHide",
    "tabs",
    "theme",
    "topSites",
    "unlimitedStorage",
    "userScripts",
    "webNavigation",
    "webRequest",
    "webRequestBlocking",
    "webRequestFilterResponse",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestError {
    NotJson,
    Missing(&'static str),
    Invalid(&'static str),
}

impl fmt::Display for ManifestError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotJson => formatter.write_str("the manifest is not valid JSON"),
            Self::Missing(key) => write!(formatter, "the manifest has no {key}"),
            Self::Invalid(key) => write!(formatter, "the manifest's {key} is not valid"),
        }
    }
}

impl std::error::Error for ManifestError {}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RunAt {
    Start,
    End,
    #[default]
    Idle,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ContentScript {
    pub matches: Vec<String>,
    pub exclude_matches: Vec<String>,
    pub js: Vec<String>,
    pub css: Vec<String>,
    pub run_at: RunAt,
    pub all_frames: bool,
    pub match_about_blank: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Background {
    Page(String),
    Scripts { scripts: Vec<String>, module: bool },
    ServiceWorker { script: String, module: bool },
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Action {
    pub title: Option<String>,
    pub popup: Option<String>,
    pub icons: BTreeMap<u32, String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebResources {
    pub paths: Vec<String>,
    pub matches: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleSet {
    pub id: String,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Shortcut {
    pub name: String,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Manifest {
    pub manifest_version: u8,
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub default_locale: Option<String>,
    pub gecko_id: Option<String>,
    pub update_url: Option<String>,
    pub permissions: Vec<String>,
    pub origins: Vec<String>,
    pub optional_permissions: Vec<String>,
    pub optional_origins: Vec<String>,
    pub unknown_permissions: Vec<String>,
    pub background: Option<Background>,
    pub content_scripts: Vec<ContentScript>,
    pub action: Option<Action>,
    pub options_page: Option<String>,
    pub web_resources: Vec<WebResources>,
    pub rule_sets: Vec<RuleSet>,
    pub commands: Vec<Shortcut>,
    pub icons: BTreeMap<u32, String>,
}

pub type Messages = BTreeMap<String, String>;

pub fn parse_messages(bytes: &[u8]) -> Messages {
    let Ok(Value::Object(entries)) = serde_json::from_slice::<Value>(&without_comments(bytes))
    else {
        return Messages::new();
    };
    entries
        .into_iter()
        .filter_map(|(key, entry)| {
            let message = entry.get("message")?.as_str()?.to_owned();
            Some((key.to_ascii_lowercase(), message))
        })
        .collect()
}

pub fn without_comments(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut position = 0;
    let mut in_string = false;
    while let Some(&byte) = bytes.get(position) {
        let next = bytes.get(position + 1).copied();
        if in_string {
            output.push(byte);
            if byte == b'\\' {
                output.extend(next);
                position += 1;
            } else if byte == b'"' {
                in_string = false;
            }
            position += 1;
        } else if byte == b'/' && next == Some(b'/') {
            position += bytes
                .get(position..)
                .and_then(|rest| rest.iter().position(|&c| c == b'\n'))
                .unwrap_or(bytes.len() - position);
        } else if byte == b'/' && next == Some(b'*') {
            let rest = bytes.get(position + 2..).unwrap_or_default();
            position += 2 + rest
                .windows(2)
                .position(|pair| pair == b"*/")
                .map_or(rest.len(), |end| end + 2);
        } else {
            in_string = byte == b'"';
            output.push(byte);
            position += 1;
        }
    }
    output
}

fn localized(text: &str, messages: &Messages) -> String {
    let Some(key) = text
        .strip_prefix("__MSG_")
        .and_then(|rest| rest.strip_suffix("__"))
    else {
        return text.to_owned();
    };
    messages
        .get(&key.to_ascii_lowercase())
        .cloned()
        .unwrap_or_else(|| text.to_owned())
}

fn text(object: &Map<String, Value>, key: &str, messages: &Messages) -> Option<String> {
    object
        .get(key)
        .and_then(Value::as_str)
        .map(|value| localized(value, messages))
}

fn strings(object: &Map<String, Value>, key: &str) -> Vec<String> {
    object
        .get(key)
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

fn package_path(path: &str) -> String {
    path.trim_start_matches('/').to_owned()
}

fn paths(object: &Map<String, Value>, key: &str) -> Vec<String> {
    strings(object, key)
        .iter()
        .map(|path| package_path(path))
        .collect()
}

fn path(object: &Map<String, Value>, key: &str) -> Option<String> {
    object.get(key).and_then(Value::as_str).map(package_path)
}

fn icons(value: Option<&Value>) -> BTreeMap<u32, String> {
    match value {
        Some(Value::String(icon)) => BTreeMap::from([(32, package_path(icon))]),
        Some(Value::Object(sizes)) => sizes
            .iter()
            .filter_map(|(size, icon)| Some((size.parse().ok()?, package_path(icon.as_str()?))))
            .collect(),
        _ => BTreeMap::new(),
    }
}

fn is_origin(permission: &str) -> bool {
    permission == "<all_urls>" || permission.contains("://")
}

struct Sorted {
    api: Vec<String>,
    origins: Vec<String>,
    unknown: Vec<String>,
}

fn sort_permissions(entries: Vec<String>) -> Sorted {
    let mut sorted = Sorted {
        api: Vec::new(),
        origins: Vec::new(),
        unknown: Vec::new(),
    };
    for entry in entries {
        if is_origin(&entry) {
            if MatchPattern::parse(&entry).is_ok() {
                sorted.origins.push(entry);
            } else {
                sorted.unknown.push(entry);
            }
        } else if API_PERMISSIONS.contains(&entry.as_str()) {
            sorted.api.push(entry);
        } else {
            sorted.unknown.push(entry);
        }
    }
    sorted
}

fn background(object: &Map<String, Value>) -> Option<Background> {
    let background = object.get("background")?.as_object()?;
    let module = background.get("type").and_then(Value::as_str) == Some("module");
    let scripts = paths(background, "scripts");
    if let Some(page) = path(background, "page") {
        Some(Background::Page(page))
    } else if !scripts.is_empty() {
        Some(Background::Scripts { scripts, module })
    } else {
        path(background, "service_worker")
            .map(|script| Background::ServiceWorker { script, module })
    }
}

fn content_script(object: &Map<String, Value>) -> Result<ContentScript, ManifestError> {
    let matches = strings(object, "matches");
    if matches.is_empty() {
        return Err(ManifestError::Invalid("content_scripts"));
    }
    let exclude_matches = strings(object, "exclude_matches");
    if matches
        .iter()
        .chain(&exclude_matches)
        .any(|pattern| MatchPattern::parse(pattern).is_err())
    {
        return Err(ManifestError::Invalid("content_scripts"));
    }
    Ok(ContentScript {
        matches,
        exclude_matches,
        js: paths(object, "js"),
        css: paths(object, "css"),
        run_at: match object.get("run_at").and_then(Value::as_str) {
            Some("document_start") => RunAt::Start,
            Some("document_end") => RunAt::End,
            _ => RunAt::Idle,
        },
        all_frames: object.get("all_frames").and_then(Value::as_bool) == Some(true),
        match_about_blank: object.get("match_about_blank").and_then(Value::as_bool) == Some(true),
    })
}

fn web_resources(object: &Map<String, Value>) -> Vec<WebResources> {
    let Some(entries) = object
        .get("web_accessible_resources")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    let flat: Vec<String> = entries
        .iter()
        .filter_map(Value::as_str)
        .map(package_path)
        .collect();
    let mut groups: Vec<WebResources> = entries
        .iter()
        .filter_map(Value::as_object)
        .map(|entry| WebResources {
            paths: paths(entry, "resources"),
            matches: strings(entry, "matches"),
        })
        .collect();
    if !flat.is_empty() {
        groups.push(WebResources {
            paths: flat,
            matches: vec!["<all_urls>".to_owned()],
        });
    }
    groups
}

fn rule_sets(object: &Map<String, Value>) -> Vec<RuleSet> {
    object
        .get("declarative_net_request")
        .and_then(|value| value.get("rule_resources"))
        .and_then(Value::as_array)
        .map(|sets| {
            sets.iter()
                .filter_map(|set| {
                    Some(RuleSet {
                        id: set.get("id")?.as_str()?.to_owned(),
                        enabled: set.get("enabled")?.as_bool()?,
                        path: package_path(set.get("path")?.as_str()?),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn parse(bytes: &[u8], messages: &Messages) -> Result<Manifest, ManifestError> {
    let value: Value =
        serde_json::from_slice(&without_comments(bytes)).map_err(|_| ManifestError::NotJson)?;
    let object = value.as_object().ok_or(ManifestError::NotJson)?;
    let manifest_version = match object.get("manifest_version").and_then(Value::as_u64) {
        Some(2) => 2,
        Some(3) => 3,
        Some(_) => return Err(ManifestError::Invalid("manifest_version")),
        None => return Err(ManifestError::Missing("manifest_version")),
    };
    let name = text(object, "name", messages)
        .filter(|name| !name.is_empty())
        .ok_or(ManifestError::Missing("name"))?;
    let version = object
        .get("version")
        .and_then(Value::as_str)
        .filter(|version| !version.is_empty())
        .ok_or(ManifestError::Missing("version"))?
        .to_owned();
    let gecko = object
        .get("browser_specific_settings")
        .or_else(|| object.get("applications"))
        .and_then(|settings| settings.get("gecko"))
        .and_then(Value::as_object);
    let required = sort_permissions(strings(object, "permissions"));
    let optional = sort_permissions(strings(object, "optional_permissions"));
    let mut origins = required.origins;
    origins.extend(strings(object, "host_permissions"));
    let mut optional_origins = optional.origins;
    optional_origins.extend(strings(object, "optional_host_permissions"));
    let mut unknown_permissions = required.unknown;
    unknown_permissions.extend(optional.unknown);
    let content_scripts = object
        .get("content_scripts")
        .and_then(Value::as_array)
        .map(|scripts| {
            scripts
                .iter()
                .map(|script| {
                    script
                        .as_object()
                        .ok_or(ManifestError::Invalid("content_scripts"))
                        .and_then(content_script)
                })
                .collect::<Result<Vec<_>, _>>()
        })
        .transpose()?
        .unwrap_or_default();
    let action = object
        .get("action")
        .or_else(|| object.get("browser_action"))
        .and_then(Value::as_object)
        .map(|action| Action {
            title: text(action, "default_title", messages),
            popup: path(action, "default_popup"),
            icons: icons(action.get("default_icon")),
        });
    let options_page = object
        .get("options_ui")
        .and_then(Value::as_object)
        .and_then(|options| path(options, "page"))
        .or_else(|| path(object, "options_page"));
    let commands = object
        .get("commands")
        .and_then(Value::as_object)
        .map(|commands| {
            commands
                .iter()
                .map(|(name, command)| Shortcut {
                    name: name.clone(),
                    description: command
                        .as_object()
                        .and_then(|command| text(command, "description", messages)),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(Manifest {
        manifest_version,
        name,
        version,
        description: text(object, "description", messages),
        default_locale: object
            .get("default_locale")
            .and_then(Value::as_str)
            .map(str::to_owned),
        gecko_id: gecko
            .and_then(|gecko| gecko.get("id"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        update_url: gecko
            .and_then(|gecko| gecko.get("update_url"))
            .or_else(|| object.get("update_url"))
            .and_then(Value::as_str)
            .map(str::to_owned),
        permissions: required.api,
        origins,
        optional_permissions: optional.api,
        optional_origins,
        unknown_permissions,
        background: background(object),
        content_scripts,
        action,
        options_page,
        web_resources: web_resources(object),
        rule_sets: rule_sets(object),
        commands,
        icons: icons(object.get("icons")),
    })
}

pub fn default_locale(bytes: &[u8]) -> Option<String> {
    serde_json::from_slice::<Value>(&without_comments(bytes))
        .ok()?
        .get("default_locale")?
        .as_str()
        .map(str::to_owned)
}
