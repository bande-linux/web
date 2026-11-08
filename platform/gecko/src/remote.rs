use std::sync::mpsc::Sender;

use zbus::blocking::{Connection, MessageIterator};
use zbus::fdo::{RequestNameFlags, RequestNameReply};
use zbus::message::Type;
use zbus::names::{BusName, InterfaceName, WellKnownName};
use zbus::zvariant::ObjectPath;

const OPEN_URL: &str = "OpenURL";

pub(crate) fn send(name: &str, path: &str, interface: &str, command: &[u8]) -> bool {
    let (Ok(name), Ok(path), Ok(interface)) = (
        BusName::try_from(name),
        ObjectPath::try_from(path),
        InterfaceName::try_from(interface),
    ) else {
        return false;
    };
    let Ok(session) = Connection::session() else {
        return false;
    };
    session
        .call_method(Some(name), path, Some(interface), OPEN_URL, &(command,))
        .is_ok()
}

pub(crate) fn serve(
    name: &str,
    path: &str,
    interface: &str,
    deliver: impl Fn(Vec<u8>) + Send + 'static,
) -> bool {
    let (Ok(name), Ok(path), Ok(interface)) = (
        WellKnownName::try_from(name.to_owned()),
        ObjectPath::try_from(path.to_owned()),
        InterfaceName::try_from(interface.to_owned()),
    ) else {
        return false;
    };
    let Ok(session) = Connection::session() else {
        return false;
    };
    let Ok(bus) = zbus::blocking::fdo::DBusProxy::new(&session) else {
        return false;
    };
    let owned = bus.request_name(name, RequestNameFlags::DoNotQueue.into());
    if !matches!(owned, Ok(RequestNameReply::PrimaryOwner | RequestNameReply::AlreadyOwner)) {
        return false;
    }
    std::thread::spawn(move || {
        for message in MessageIterator::from(&session) {
            let Ok(message) = message else {
                continue;
            };
            let header = message.header();
            if header.message_type() != Type::MethodCall
                || header.path() != Some(&path)
                || header.member().map(|member| member.as_str()) != Some(OPEN_URL)
                || header.interface().is_some_and(|called| *called != interface)
            {
                continue;
            }
            match message.body().deserialize::<Vec<u8>>() {
                Ok(command) if !command.is_empty() => {
                    deliver(command);
                    let _ = session.reply(&header, &());
                }
                _ => {
                    let _ = session.reply_error(
                        &header,
                        "org.freedesktop.DBus.Error.InvalidArgs",
                        &"OpenURL wants the command line",
                    );
                }
            }
        }
    });
    true
}

pub(crate) fn queue(sender: Sender<crate::Queued>, wake: std::os::unix::net::UnixStream) -> impl Fn(Vec<u8>) + Send + 'static {
    move |command| {
        crate::answer(
            &sender,
            &wake,
            crate::Queued {
                kind: crate::EVENT_REMOTE,
                data: command,
                ..crate::Queued::default()
            },
        );
    }
}
