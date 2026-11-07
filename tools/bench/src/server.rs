use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};

pub type Report = (HashMap<String, f64>, Sender<()>);

const INJECTED: &str = include_str!("injected.js");

fn content_type(path: &str) -> &'static str {
    let name = path.split('?').next().unwrap_or(path);
    match name.rsplit('.').next().unwrap_or_default() {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css",
        "js" => "text/javascript",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "avif" => "image/avif",
        "svg" => "image/svg+xml",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        _ => "application/octet-stream",
    }
}

fn decode(path: &str) -> Vec<u8> {
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while let Some(&byte) = bytes.get(index) {
        let hex = (byte == b'%')
            .then(|| bytes.get(index + 1..index + 3))
            .flatten()
            .and_then(|pair| std::str::from_utf8(pair).ok())
            .and_then(|pair| u8::from_str_radix(pair, 16).ok());
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

fn query(text: &str) -> HashMap<String, f64> {
    text.split('&')
        .filter_map(|pair| pair.split_once('='))
        .filter_map(|(key, value)| Some((key.to_owned(), value.parse().ok()?)))
        .collect()
}

fn respond(stream: &mut TcpStream, status: &str, kind: &str, body: &[u8]) -> std::io::Result<()> {
    write!(
        stream,
        "HTTP/1.1 {status}\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: keep-alive\r\n\r\n",
        body.len()
    )?;
    stream.write_all(body)
}

fn find(root: &Path, target: &str) -> Option<PathBuf> {
    let (path, parameters) = target.split_once('?').unwrap_or((target, ""));
    let relative = String::from_utf8_lossy(&decode(path)).into_owned();
    let relative = relative.trim_start_matches('/');
    if relative.contains("..") {
        return None;
    }
    let with_query = root.join(format!("{relative}?{parameters}"));
    if !parameters.is_empty() && with_query.is_file() {
        return Some(with_query);
    }
    Some(root.join(relative)).filter(|file| file.is_file())
}

fn connection(mut stream: TcpStream, root: &Path, reports: &Sender<Report>) -> std::io::Result<()> {
    let _ = stream.set_nodelay(true);
    let mut reader = BufReader::new(stream.try_clone()?);
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(());
        }
        let target = line.split_whitespace().nth(1).unwrap_or("/").to_owned();
        loop {
            let mut header = String::new();
            if reader.read_line(&mut header)? == 0 {
                return Ok(());
            }
            if header.trim().is_empty() {
                break;
            }
        }
        if let Some(parameters) = target.strip_prefix("/__report?") {
            let (done, wait) = channel();
            if reports.send((query(parameters), done)).is_ok() {
                let _ = wait.recv();
            }
            respond(&mut stream, "200 OK", "text/plain", b"ok")?;
            continue;
        }
        let measured = target.contains("report=");
        match find(root, &target).and_then(|file| std::fs::read(file).ok()) {
            Some(mut body) => {
                if measured {
                    body.extend_from_slice(b"<script>");
                    body.extend_from_slice(INJECTED.as_bytes());
                    body.extend_from_slice(b"</script>");
                }
                respond(&mut stream, "200 OK", content_type(&target), &body)?;
            }
            None => respond(&mut stream, "404 Not Found", "text/plain", b"")?,
        }
    }
}

pub fn start(root: PathBuf) -> std::io::Result<(u16, Receiver<Report>)> {
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let port = listener.local_addr()?.port();
    let (sender, reports) = channel();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            let root = root.clone();
            let sender = sender.clone();
            std::thread::spawn(move || connection(stream, &root, &sender));
        }
    });
    Ok((port, reports))
}
