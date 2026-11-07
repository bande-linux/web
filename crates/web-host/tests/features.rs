use std::io::{BufRead, BufReader, Read, Seek, Write};
use std::net::TcpListener;
use std::os::fd::OwnedFd;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use rustix::event::{PollFd, PollFlags, poll};
use rustix::time::Timespec;
use web_host::{
    Command, Helpers, Host, LoadId, Outcome, Program, Session, Update, Url, ViewId, Viewport,
};

const FILE_BYTES: usize = 3 << 20;

fn article() -> String {
    let paragraph = "The needle sits in this long paragraph, which goes on for a while so that \
                     the reader finds enough text to call the page an article of some worth. ";
    let body = format!("<p>{}</p>", paragraph.repeat(3)).repeat(8);
    format!(
        "<!doctype html><title>Story</title><nav><a href=/>Home</a> needle-nav</nav>\
         <main><h1>Story</h1>{body}</main><footer>Footer text</footer>"
    )
}

fn serve(listener: TcpListener) {
    for stream in listener.incoming().flatten() {
        let mut reader = BufReader::new(&stream);
        let mut request = String::new();
        if reader.read_line(&mut request).is_err() {
            continue;
        }
        let mut line = String::new();
        while reader.read_line(&mut line).is_ok_and(|count| count > 2) {
            line.clear();
        }
        let (headers, body): (&str, Vec<u8>) = if request.contains("/plain") {
            ("Content-Type: text/plain", b"plain <text>".to_vec())
        } else if request.contains("/file") {
            (
                "Content-Type: application/zip\r\nContent-Disposition: attachment; filename*=UTF-8''caf%C3%A9.zip",
                vec![7; FILE_BYTES],
            )
        } else {
            ("Content-Type: text/html", article().into_bytes())
        };
        let _ = write!(
            &stream,
            "HTTP/1.1 200 OK\r\n{headers}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        );
        let _ = (&stream).write_all(&body);
    }
}

fn program(role: &str) -> Result<Program, String> {
    Ok(Program {
        path: std::env::current_exe().map_err(|error| error.to_string())?,
        arguments: vec![role.to_owned()],
    })
}

fn wait<T>(
    host: &Host,
    pending: &mut Vec<Update>,
    mut wanted: impl FnMut(Update) -> Option<T>,
) -> Result<T, String> {
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        pending.extend(host.updates());
        while !pending.is_empty() {
            if let Some(found) = wanted(pending.remove(0)) {
                return Ok(found);
            }
        }
        if Instant::now() > deadline {
            return Err("timed out".into());
        }
        let timeout = Timespec {
            tv_sec: 1,
            tv_nsec: 0,
        };
        let fd = host.fd();
        let mut fds = [PollFd::new(&fd, PollFlags::IN)];
        poll(&mut fds, Some(&timeout)).map_err(|error| error.to_string())?;
    }
}

fn scenario() -> Result<(), String> {
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|error| error.to_string())?;
    let port = listener
        .local_addr()
        .map_err(|error| error.to_string())?
        .port();
    std::thread::spawn(move || serve(listener));
    let url = |path: &str| {
        Url::parse(&format!("http://127.0.0.1:{port}{path}")).map_err(|error| error.to_string())
    };
    let host = Host::new(
        Helpers {
            renderer: program("renderer")?,
            network: program("network")?,
            unresponsive_after: Duration::from_secs(30),
        },
        Box::new(|_| {}),
    )
    .map_err(|error| error.to_string())?;
    let link = host.link();
    let view = ViewId(1);
    let load = |load: u64, url: Url| Command::Load {
        view,
        load: LoadId(load),
        url,
        viewport: Viewport {
            width: 800.0,
            height: 600.0,
        },
        session: Session::Default,
    };
    let mut pending = Vec::new();
    link.send(load(1, url("/story")?));
    let readable = wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Frame(frame) => Some(frame.readable),
        _ => None,
    })?;
    if !readable {
        return Err("an article was not offered in Reader".into());
    }
    let find = |load: u64, query: &str| Command::Find {
        view,
        load: LoadId(load),
        query: query.to_owned(),
    };
    link.send(find(1, "NEEDLE"));
    let found = wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Found(found) => Some(found),
        _ => None,
    })?;
    if found.len() != 25 || found.iter().flatten().any(|rect| rect.width <= 0.0) {
        return Err(format!("Find saw {} matches", found.len()));
    }
    link.send(Command::Reader {
        view,
        load: LoadId(2),
        style: "body { color: black }".to_owned(),
    });
    wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Frame(frame) if update.load == LoadId(2) => (!frame.readable).then_some(()),
        _ => None,
    })?;
    link.send(find(2, "needle"));
    let found = wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Found(found) => Some(found),
        _ => None,
    })?;
    if found.len() != 24 {
        return Err(format!(
            "Reader kept {} matches, not the article's",
            found.len()
        ));
    }
    link.send(load(3, url("/plain")?));
    wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Frame(_) if update.load == LoadId(3) => Some(()),
        _ => None,
    })?;
    link.send(load(4, url("/file.zip")?));
    let offer = wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Offered(offer) => Some(offer),
        _ => None,
    })?;
    if offer.name.as_deref() != Some("café.zip") || offer.length != Some(FILE_BYTES as u64) {
        return Err(format!("the offer was {:?} {:?}", offer.name, offer.length));
    }
    let directory = std::env::temp_dir().join(format!("web-host-features-{}", std::process::id()));
    std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    let path = directory.join("saved");
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let sent = file.try_clone().map_err(|error| error.to_string())?;
    link.send(Command::Save {
        download: offer.download,
        file: OwnedFd::from(sent),
    });
    let error = wait(&host, &mut pending, |update| match update.outcome {
        Outcome::Saved { error, .. } => Some(error),
        _ => None,
    })?;
    if let Some(error) = error {
        return Err(error);
    }
    file.rewind().map_err(|error| error.to_string())?;
    let mut saved = Vec::new();
    file.read_to_end(&mut saved)
        .map_err(|error| error.to_string())?;
    let _ = std::fs::remove_dir_all(&directory);
    if saved.len() != FILE_BYTES || saved.iter().any(|byte| *byte != 7) {
        return Err(format!("{} bytes were saved", saved.len()));
    }
    Ok(())
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("renderer") => return web_renderer::main(),
        Some("network") => return web_netd::main(),
        _ => {}
    }
    match scenario() {
        Ok(()) => {
            println!("features: Find, Reader, plain text and saving a download work");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("FAIL: {error}");
            ExitCode::FAILURE
        }
    }
}
