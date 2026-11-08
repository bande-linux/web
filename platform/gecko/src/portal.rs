use std::collections::HashMap;
use std::sync::atomic::{AtomicU32, Ordering};

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const FILE_CHOOSER: &str = "org.freedesktop.portal.FileChooser";
const REQUEST: &str = "org.freedesktop.portal.Request";
const MODE_SAVE: u32 = 1;
const MODE_FOLDER: u32 = 2;
const MODE_MULTIPLE: u32 = 3;
const GLOB: u32 = 0;

pub(crate) struct Question {
    pub(crate) mode: u32,
    pub(crate) title: String,
    pub(crate) filters: String,
    pub(crate) filter_index: u32,
    pub(crate) name: String,
    pub(crate) folder: String,
}

type Filter = (String, Vec<(u32, String)>);

fn filters_of(text: &str) -> Vec<Filter> {
    text.lines()
        .filter_map(|line| {
            let (title, patterns) = line.split_once('\t')?;
            let patterns: Vec<(u32, String)> = patterns
                .split(';')
                .map(str::trim)
                .filter(|pattern| !pattern.is_empty())
                .map(|pattern| (GLOB, pattern.to_owned()))
                .collect();
            (!patterns.is_empty()).then(|| (title.to_owned(), patterns))
        })
        .collect()
}

fn folder_bytes(folder: &str) -> Vec<u8> {
    let mut bytes = folder.as_bytes().to_vec();
    bytes.push(0);
    bytes
}

pub(crate) fn ask(parent: &str, question: &Question) -> Option<Vec<String>> {
    static REQUESTS: AtomicU32 = AtomicU32::new(0);
    let session = Connection::session().ok()?;
    let token = format!(
        "bande_browser_{}_{}",
        std::process::id(),
        REQUESTS.fetch_add(1, Ordering::Relaxed)
    );
    let sender = session.unique_name()?.trim_start_matches(':').replace('.', "_");
    let path = OwnedObjectPath::try_from(format!("{PORTAL_PATH}/request/{sender}/{token}")).ok()?;
    let request = Proxy::new(&session, PORTAL, path, REQUEST).ok()?;
    let mut responses = request.receive_signal("Response").ok()?;
    let chooser = Proxy::new(&session, PORTAL, PORTAL_PATH, FILE_CHOOSER).ok()?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.clone()));
    options.insert("modal", Value::from(true));
    let filters = filters_of(&question.filters);
    if let Some(current) = filters.get(question.filter_index as usize) {
        options.insert("current_filter", Value::from(current.clone()));
    }
    if !filters.is_empty() {
        options.insert("filters", Value::from(filters));
    }
    if !question.folder.is_empty() {
        options.insert("current_folder", Value::from(folder_bytes(&question.folder)));
    }
    let method = match question.mode {
        MODE_SAVE => {
            options.insert("current_name", Value::from(question.name.clone()));
            "SaveFile"
        }
        mode => {
            options.insert("multiple", Value::from(mode == MODE_MULTIPLE));
            options.insert("directory", Value::from(mode == MODE_FOLDER));
            "OpenFile"
        }
    };
    let _: OwnedObjectPath = chooser
        .call(method, &(parent, question.title.as_str(), options))
        .ok()?;
    let response = responses.next()?;
    let (code, results) = response
        .body()
        .deserialize::<(u32, HashMap<String, OwnedValue>)>()
        .ok()?;
    if code != 0 {
        return None;
    }
    results
        .get("uris")
        .and_then(|uris| Vec::<String>::try_from(uris.try_clone().ok()?).ok())
}
