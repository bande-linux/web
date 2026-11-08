use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::sync::atomic::{AtomicU32, Ordering};

use futures_lite::{StreamExt, future};
use zbus::proxy::SignalStream;
use zbus::zvariant::{OwnedObjectPath, OwnedValue, Value};
use zbus::{Connection, Proxy};

const PORTAL: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
const SCREEN_CAST: &str = "org.freedesktop.portal.ScreenCast";
const CAMERA: &str = "org.freedesktop.portal.Camera";

pub(crate) const SUCCESS: u32 = 0;
pub(crate) const FAILED: u32 = 2;

const CURSOR_HIDDEN: u32 = 1;
const CURSOR_EMBEDDED: u32 = 2;
const CURSOR_METADATA: u32 = 4;

pub(crate) struct Capture {
    pub(crate) sources: u32,
    pub(crate) cursor: u32,
    pub(crate) persist: u32,
    pub(crate) restore_token: String,
}

pub(crate) enum Report {
    Answer {
        response: u32,
        node: u32,
        fd: Option<OwnedFd>,
        restore_token: String,
    },
    Closed,
}

type Results = HashMap<String, OwnedValue>;

struct Portal {
    connection: Connection,
    sender: String,
}

impl Portal {
    async fn open() -> zbus::Result<Self> {
        let connection = Connection::session().await?;
        let sender = connection
            .unique_name()
            .map(|name| name.trim_start_matches(':').replace('.', "_"))
            .unwrap_or_default();
        Ok(Self { connection, sender })
    }

    fn token() -> String {
        static TOKENS: AtomicU32 = AtomicU32::new(0);
        format!(
            "bande_browser_cast_{}_{}",
            std::process::id(),
            TOKENS.fetch_add(1, Ordering::Relaxed)
        )
    }

    async fn proxy(&self, path: OwnedObjectPath, interface: &'static str) -> zbus::Result<Proxy<'static>> {
        Proxy::new(&self.connection, PORTAL, path, interface).await
    }

    async fn responses(&self, token: &str) -> zbus::Result<SignalStream<'static>> {
        let path = OwnedObjectPath::try_from(format!("{PORTAL_PATH}/request/{}/{token}", self.sender))?;
        let request = self.proxy(path, REQUEST).await?;
        request.receive_signal("Response").await
    }
}

async fn answered(responses: &mut SignalStream<'static>) -> Result<Results, u32> {
    let message = responses.next().await.ok_or(FAILED)?;
    let (response, results) = message
        .body()
        .deserialize::<(u32, Results)>()
        .map_err(|_| FAILED)?;
    match response {
        SUCCESS => Ok(results),
        refused => Err(refused),
    }
}

fn failed<T>(_: T) -> u32 {
    FAILED
}

fn session_of(results: &Results) -> Option<OwnedObjectPath> {
    let value = results.get("session_handle")?.try_clone().ok()?;
    match String::try_from(value.try_clone().ok()?) {
        Ok(text) => OwnedObjectPath::try_from(text).ok(),
        Err(_) => OwnedObjectPath::try_from(value).ok(),
    }
}

fn first_node(streams: &Value<'_>) -> Option<u32> {
    let Value::Array(streams) = streams else {
        return None;
    };
    streams.iter().find_map(|stream| match stream {
        Value::Structure(stream) => match stream.fields().first() {
            Some(Value::U32(node)) => Some(*node),
            _ => None,
        },
        _ => None,
    })
}

fn cursor_for(preferred: u32, available: u32) -> u32 {
    [preferred, CURSOR_METADATA, CURSOR_EMBEDDED, CURSOR_HIDDEN]
        .into_iter()
        .find(|mode| available & mode != 0)
        .unwrap_or(0)
}

struct Started {
    node: u32,
    fd: OwnedFd,
    restore_token: String,
}

async fn negotiate(
    portal: &Portal,
    cast: &Proxy<'static>,
    session: &OwnedObjectPath,
    capture: &Capture,
) -> Result<Started, u32> {
    let version: u32 = cast.get_property("version").await.unwrap_or(1);
    let available: u32 = cast.get_property("AvailableCursorModes").await.unwrap_or(0);
    let token = Portal::token();
    let mut responses = portal.responses(&token).await.map_err(failed)?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    options.insert("types", Value::from(capture.sources));
    options.insert("multiple", Value::from(false));
    let cursor = cursor_for(capture.cursor, available);
    if cursor != 0 {
        options.insert("cursor_mode", Value::from(cursor));
    }
    if version >= 4 && capture.persist != 0 {
        options.insert("persist_mode", Value::from(capture.persist));
        if !capture.restore_token.is_empty() {
            options.insert("restore_token", Value::from(capture.restore_token.as_str()));
        }
    }
    let _: OwnedObjectPath = cast
        .call("SelectSources", &(session, options))
        .await
        .map_err(failed)?;
    answered(&mut responses).await?;

    let token = Portal::token();
    let mut responses = portal.responses(&token).await.map_err(failed)?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let _: OwnedObjectPath = cast
        .call("Start", &(session, "", options))
        .await
        .map_err(failed)?;
    let results = answered(&mut responses).await?;
    let node = results.get("streams").and_then(|streams| first_node(streams)).ok_or(FAILED)?;
    let restore_token = results
        .get("restore_token")
        .and_then(|token| token.try_clone().ok())
        .and_then(|token| String::try_from(token).ok())
        .unwrap_or_default();

    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let fd: zbus::zvariant::OwnedFd = cast
        .call("OpenPipeWireRemote", &(session, options))
        .await
        .map_err(failed)?;
    Ok(Started {
        node,
        fd: OwnedFd::from(fd),
        restore_token,
    })
}

async fn close(portal: &Portal, session: OwnedObjectPath) {
    if let Ok(proxy) = portal.proxy(session, SESSION).await {
        let _: zbus::Result<()> = proxy.call("Close", &()).await;
    }
}

async fn cast(capture: Capture, stop: async_channel::Receiver<()>, report: &mut impl FnMut(Report)) {
    let refused = |response| Report::Answer {
        response,
        node: 0,
        fd: None,
        restore_token: String::new(),
    };
    let Ok(portal) = Portal::open().await else {
        report(refused(FAILED));
        return;
    };
    let Ok(screen_cast) = Proxy::new(&portal.connection, PORTAL, PORTAL_PATH, SCREEN_CAST).await else {
        report(refused(FAILED));
        return;
    };
    let token = Portal::token();
    let created = async {
        let mut responses = portal.responses(&token).await.map_err(failed)?;
        let mut options: HashMap<&str, Value<'_>> = HashMap::new();
        options.insert("handle_token", Value::from(token.as_str()));
        options.insert("session_handle_token", Value::from(Portal::token()));
        let _: OwnedObjectPath = screen_cast
            .call("CreateSession", &(options,))
            .await
            .map_err(failed)?;
        session_of(&answered(&mut responses).await?).ok_or(FAILED)
    };
    let session = match future::or(created, async {
        let _ = stop.recv().await;
        Err(FAILED)
    })
    .await
    {
        Ok(session) => session,
        Err(response) => {
            report(refused(response));
            return;
        }
    };
    let Ok(session_proxy) = portal.proxy(session.clone(), SESSION).await else {
        report(refused(FAILED));
        close(&portal, session).await;
        return;
    };
    let Ok(mut closed) = session_proxy.receive_signal("Closed").await else {
        report(refused(FAILED));
        close(&portal, session).await;
        return;
    };
    let negotiated = future::or(
        async { Some(negotiate(&portal, &screen_cast, &session, &capture).await) },
        async {
            let _ = stop.recv().await;
            None
        },
    )
    .await;
    let started = match negotiated {
        Some(Ok(started)) => started,
        Some(Err(response)) => {
            report(refused(response));
            close(&portal, session).await;
            return;
        }
        None => {
            close(&portal, session).await;
            return;
        }
    };
    report(Report::Answer {
        response: SUCCESS,
        node: started.node,
        fd: Some(started.fd),
        restore_token: started.restore_token,
    });
    let ended_by_portal = future::or(
        async {
            closed.next().await;
            true
        },
        async {
            let _ = stop.recv().await;
            false
        },
    )
    .await;
    if ended_by_portal {
        report(Report::Closed);
    } else {
        close(&portal, session).await;
    }
}

pub(crate) fn screen_cast(capture: Capture, stop: async_channel::Receiver<()>, mut report: impl FnMut(Report)) {
    zbus::block_on(cast(capture, stop, &mut report));
}

async fn camera_access() -> Result<OwnedFd, u32> {
    let portal = Portal::open().await.map_err(failed)?;
    let camera = Proxy::new(&portal.connection, PORTAL, PORTAL_PATH, CAMERA)
        .await
        .map_err(failed)?;
    let token = Portal::token();
    let mut responses = portal.responses(&token).await.map_err(failed)?;
    let mut options: HashMap<&str, Value<'_>> = HashMap::new();
    options.insert("handle_token", Value::from(token.as_str()));
    let _: OwnedObjectPath = camera
        .call("AccessCamera", &(options,))
        .await
        .map_err(failed)?;
    answered(&mut responses).await?;
    let options: HashMap<&str, Value<'_>> = HashMap::new();
    let fd: zbus::zvariant::OwnedFd = camera
        .call("OpenPipeWireRemote", &(options,))
        .await
        .map_err(failed)?;
    Ok(OwnedFd::from(fd))
}

pub(crate) fn camera(stop: async_channel::Receiver<()>) -> Result<OwnedFd, u32> {
    zbus::block_on(future::or(camera_access(), async {
        let _ = stop.recv().await;
        Err(FAILED)
    }))
}
