use std::collections::HashMap;

use zbus::blocking::{Connection, Proxy};
use zbus::zvariant::{OwnedValue, Value};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const NOTIFICATION: &str = "org.freedesktop.portal.Notification";
const REGISTRY: &str = "org.freedesktop.host.portal.Registry";
const APP_ID: &str = "org.bande.Browser";

pub(crate) struct Notice<'a> {
    pub(crate) id: &'a str,
    pub(crate) title: &'a str,
    pub(crate) body: &'a str,
    pub(crate) icon: &'a [u8],
    pub(crate) buttons: Vec<(String, String)>,
    pub(crate) silent: bool,
    pub(crate) persistent: bool,
}

pub(crate) struct Notifier {
    proxy: Proxy<'static>,
}

fn activation_token(parameters: &[OwnedValue]) -> String {
    parameters
        .iter()
        .filter_map(|parameter| HashMap::<String, OwnedValue>::try_from(parameter.try_clone().ok()?).ok())
        .find_map(|platform| {
            String::try_from(platform.get("activation-token")?.try_clone().ok()?).ok()
        })
        .unwrap_or_default()
}

impl Notifier {
    pub(crate) fn open(deliver: impl Fn(String, String, String) + Send + 'static) -> Option<Self> {
        let session = Connection::session().ok()?;
        let options: HashMap<&str, Value<'_>> = HashMap::new();
        let _ = session.call_method(
            Some(PORTAL),
            PORTAL_PATH,
            Some(REGISTRY),
            "Register",
            &(APP_ID, options),
        );
        let proxy = Proxy::new(&session, PORTAL, PORTAL_PATH, NOTIFICATION).ok()?;
        let invoked = proxy.receive_signal("ActionInvoked").ok()?;
        std::thread::spawn(move || {
            for message in invoked {
                if let Ok((id, action, parameters)) = message
                    .body()
                    .deserialize::<(String, String, Vec<OwnedValue>)>()
                {
                    deliver(id, action, activation_token(&parameters));
                }
            }
        });
        Some(Self { proxy })
    }

    pub(crate) fn show(&self, notice: &Notice<'_>) -> bool {
        let mut notification: HashMap<&str, Value<'_>> = HashMap::new();
        notification.insert("title", Value::from(notice.title));
        notification.insert("body", Value::from(notice.body));
        notification.insert("default-action", Value::from("default"));
        if !notice.icon.is_empty() {
            notification.insert("icon", Value::from(("bytes", Value::from(notice.icon))));
        }
        if notice.silent {
            notification.insert("sound", Value::from("silent"));
        }
        if notice.persistent {
            notification.insert("display-hint", Value::from(vec!["persistent"]));
        }
        let buttons: Vec<HashMap<&str, Value<'_>>> = notice
            .buttons
            .iter()
            .map(|(label, action)| {
                HashMap::from([
                    ("label", Value::from(label.as_str())),
                    ("action", Value::from(action.as_str())),
                ])
            })
            .collect();
        if !buttons.is_empty() {
            notification.insert("buttons", Value::from(buttons));
        }
        self.proxy
            .call::<_, _, ()>("AddNotification", &(notice.id, notification))
            .is_ok()
    }

    pub(crate) fn remove(&self, id: &str) {
        let _ = self.proxy.call::<_, _, ()>("RemoveNotification", &(id,));
    }
}
