use crate::manifest::Manifest;
use crate::pattern::MatchPattern;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    AllSites,
    Sites(Vec<String>),
    ChangeRequests,
    Tabs,
    Navigation,
    Cookies,
    History,
    Bookmarks,
    Downloads,
    Clipboard,
    Notifications,
    Management,
    Privacy,
    Proxy,
    NativeApps,
    Scripts,
    Geolocation,
}

const API_NOTICES: &[(&str, Notice)] = &[
    ("webRequestBlocking", Notice::ChangeRequests),
    ("declarativeNetRequest", Notice::ChangeRequests),
    ("tabs", Notice::Tabs),
    ("webNavigation", Notice::Navigation),
    ("cookies", Notice::Cookies),
    ("history", Notice::History),
    ("topSites", Notice::History),
    ("bookmarks", Notice::Bookmarks),
    ("downloads", Notice::Downloads),
    ("clipboardRead", Notice::Clipboard),
    ("notifications", Notice::Notifications),
    ("management", Notice::Management),
    ("privacy", Notice::Privacy),
    ("browserSettings", Notice::Privacy),
    ("proxy", Notice::Proxy),
    ("nativeMessaging", Notice::NativeApps),
    ("userScripts", Notice::Scripts),
    ("geolocation", Notice::Geolocation),
];

pub fn notices(manifest: &Manifest) -> Vec<Notice> {
    let mut sites: Vec<String> = manifest
        .origins
        .iter()
        .chain(
            manifest
                .content_scripts
                .iter()
                .flat_map(|script| &script.matches),
        )
        .cloned()
        .collect();
    sites.sort();
    sites.dedup();
    let mut found = Vec::new();
    let every_site = sites.iter().any(|site| {
        MatchPattern::parse(site)
            .map(|pattern| pattern.is_every_site())
            .unwrap_or(false)
    });
    if every_site {
        found.push(Notice::AllSites);
    } else if !sites.is_empty() {
        found.push(Notice::Sites(sites));
    }
    for (permission, notice) in API_NOTICES {
        if manifest.permissions.iter().any(|name| name == permission) && !found.contains(notice) {
            found.push(notice.clone());
        }
    }
    found
}
