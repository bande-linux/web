use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use rustix::event::{PollFd, PollFlags, poll};
use rustix::time::Timespec;
use web_host::{Helpers, Host, LoadId, Outcome, Program, Session, Url, ViewId, Viewport};

use crate::{HEIGHT, WIDTH};

fn helper(name: &str) -> std::io::Result<Program> {
    let executable = std::env::current_exe()?;
    let directory = executable
        .parent()
        .ok_or(std::io::ErrorKind::NotFound)?;
    Ok(Program {
        path: directory.join(name),
        arguments: Vec::new(),
    })
}

pub fn epoch_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |elapsed| elapsed.as_secs_f64() * 1e3)
}

fn wait(host: &Host, timeout: Duration) -> bool {
    let mut fds = [PollFd::from_borrowed_fd(host.fd(), PollFlags::IN)];
    let timeout = Timespec {
        tv_sec: timeout.as_secs() as i64,
        tv_nsec: i64::from(timeout.subsec_nanos()),
    };
    matches!(poll(&mut fds, Some(&timeout)), Ok(count) if count > 0)
}

struct Loaded {
    first_paint: f64,
    done: f64,
}

fn load(host: &Host, url: &Url, load: u64) -> std::io::Result<Loaded> {
    let view = ViewId(1);
    let mut first_paint = None;
    let mut done = 0.0;
    host.link().send(web_host::Command::Load {
        view,
        load: LoadId(load),
        url: url.clone(),
        viewport: Viewport {
            width: f32::from(WIDTH),
            height: f32::from(HEIGHT),
        },
        session: Session::Default,
    });
    let quiet = Duration::from_millis(400);
    while wait(host, if first_paint.is_some() { quiet } else { Duration::from_secs(60) }) {
        for update in host.updates() {
            match update.outcome {
                Outcome::Frame(frame) => {
                    let pixels = frame.list.rasterize(0.0, WIDTH, HEIGHT);
                    drop(pixels);
                    let now = epoch_ms();
                    first_paint.get_or_insert(now);
                    done = now;
                }
                Outcome::Relaid(list, _, _) => {
                    let pixels = list.rasterize(0.0, WIDTH, HEIGHT);
                    drop(pixels);
                    done = epoch_ms();
                }
                Outcome::Failed { message, .. } => {
                    return Err(std::io::Error::other(message));
                }
                Outcome::Crashed { url } => {
                    return Err(std::io::Error::other(format!("crashed at {url}")));
                }
                _ => {}
            }
        }
    }
    let first_paint = first_paint.ok_or(std::io::ErrorKind::TimedOut)?;
    Ok(Loaded { first_paint, done })
}

pub fn child(arguments: &[String]) -> std::io::Result<()> {
    let [url, warm] = arguments else {
        return Err(std::io::ErrorKind::InvalidInput.into());
    };
    let url = Url::parse(url).map_err(|_| std::io::ErrorKind::InvalidInput)?;
    let warm: u64 = warm.parse().map_err(|_| std::io::ErrorKind::InvalidInput)?;
    let helpers = Helpers {
        renderer: helper("web-renderer")?,
        network: helper("web-netd")?,
        unresponsive_after: Duration::from_secs(60),
    };
    let host = Host::new(helpers, Box::new(|_| {}))?;
    let first = load(&host, &url, 1)?;
    println!("cold {:.3} {:.3}", first.first_paint, first.done);
    for pid in crate::compare::tree(std::process::id()) {
        let (anon, rss) = crate::compare::memory(pid);
        let renderer = Path::new("/proc")
            .join(pid.to_string())
            .join("comm");
        let name = std::fs::read_to_string(renderer).unwrap_or_default();
        println!("memory {} {anon} {rss}", name.trim());
    }
    for round in 0..warm {
        let start = epoch_ms();
        let loaded = load(&host, &url, round + 2)?;
        println!(
            "warm {:.3} {:.3}",
            loaded.first_paint - start,
            loaded.done - start
        );
    }
    Ok(())
}
