use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use rustix::process::{Pid, Signal, kill_process};

use crate::load::epoch_ms;
use crate::server::{self, Report};
use crate::{HEIGHT, Options, WIDTH, median};

#[derive(Default, Clone)]
struct Row {
    cold_first: f64,
    cold_done: f64,
    warm_first: f64,
    warm_done: f64,
    parse: f64,
    restyle: f64,
    relayout: f64,
    scroll50: f64,
    scroll99: f64,
    total_anon: u64,
    total_rss: u64,
    tab_anon: u64,
}

const WARM: usize = 5;
const SETTLE: Duration = Duration::from_millis(1500);
const REPORT_WAIT: Duration = Duration::from_secs(120);

fn ppid(pid: u32) -> Option<u32> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let after = stat.rsplit_once(')')?.1;
    after.split_whitespace().nth(1)?.parse().ok()
}

pub fn tree(root: u32) -> Vec<u32> {
    let mut all: Vec<(u32, u32)> = Vec::new();
    for entry in std::fs::read_dir("/proc").into_iter().flatten().flatten() {
        if let Some(pid) = entry.file_name().to_str().and_then(|name| name.parse().ok())
            && let Some(parent) = ppid(pid)
        {
            all.push((pid, parent));
        }
    }
    let mut found = vec![root];
    let mut index = 0;
    while let Some(&current) = found.get(index) {
        found.extend(
            all.iter()
                .filter(|(_, parent)| *parent == current)
                .map(|(pid, _)| *pid),
        );
        index += 1;
    }
    found
}

pub fn memory(pid: u32) -> (u64, u64) {
    let text = std::fs::read_to_string(format!("/proc/{pid}/status")).unwrap_or_default();
    let field = |name: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(name))
            .and_then(|value| value.trim().trim_end_matches("kB").trim().parse().ok())
            .unwrap_or(0)
    };
    (field("RssAnon:"), field("VmRSS:"))
}

fn kill_tree(child: &mut Child) {
    for pid in tree(child.id()).into_iter().rev() {
        if let Some(pid) = Pid::from_raw(pid as i32) {
            let _ = kill_process(pid, Signal::KILL);
        }
    }
    let _ = child.wait();
}

fn tab_process(pid: u32) -> bool {
    let line = std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
    let line = String::from_utf8_lossy(&line);
    (line.contains("-contentproc") && line.contains("tab")) || line.contains("--type=renderer")
}

fn firefox_profile(directory: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(directory)?;
    std::fs::write(
        directory.join("user.js"),
        format!(
            "user_pref(\"browser.aboutwelcome.enabled\", false);
user_pref(\"browser.shell.checkDefaultBrowser\", false);
user_pref(\"browser.startup.homepage_override.mstone\", \"ignore\");
user_pref(\"startup.homepage_welcome_url\", \"\");
user_pref(\"datareporting.policy.dataSubmissionEnabled\", false);
user_pref(\"toolkit.telemetry.reportingpolicy.firstRun\", false);
user_pref(\"privacy.reduceTimerPrecision\", false);
user_pref(\"layout.frame_rate\", 0);
user_pref(\"browser.sessionstore.resume_from_crash\", false);
user_pref(\"app.update.enabled\", false);
user_pref(\"browser.safebrowsing.malware.enabled\", false);
user_pref(\"browser.safebrowsing.phishing.enabled\", false);
user_pref(\"browser.newtabpage.enabled\", false);
user_pref(\"extensions.getAddons.cache.enabled\", false);
user_pref(\"privacy.resistFingerprinting\", false);
"
        ),
    )
}

fn browser(name: &str, profile: &Path, url: &str) -> std::io::Result<Child> {
    let mut command = match name {
        "firefox" => {
            let mut command = Command::new("firefox-esr");
            command
                .args(["--headless", "--no-remote", "--profile"])
                .arg(profile)
                .args(["--width", &WIDTH.to_string(), "--height", &HEIGHT.to_string(), url]);
            command
        }
        _ => {
            let mut command = Command::new("chromium");
            command
                .args([
                    "--headless=new",
                    "--no-first-run",
                    "--disable-background-networking",
                    "--disable-sync",
                    "--disable-frame-rate-limit",
                    "--disable-gpu-vsync",
                ])
                .arg(format!("--user-data-dir={}", profile.display()))
                .arg(format!("--window-size={WIDTH},{HEIGHT}"))
                .arg(url);
            command
        }
    };
    command
        .env("HOME", profile)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}

fn next(reports: &Receiver<Report>) -> std::io::Result<Report> {
    reports
        .recv_timeout(REPORT_WAIT)
        .map_err(|_| std::io::ErrorKind::TimedOut.into())
}

fn value(report: &Report, key: &str) -> f64 {
    report.0.get(key).copied().unwrap_or(f64::NAN)
}

fn measure_browser(
    name: &str,
    page: &str,
    port: u16,
    reports: &Receiver<Report>,
    profile: &Path,
    rounds: usize,
) -> std::io::Result<Row> {
    let base = format!("http://127.0.0.1:{port}/{page}/index.html");
    let mut cold_first = Vec::new();
    let mut cold_done = Vec::new();
    for _ in 0..rounds {
        let start = epoch_ms();
        let mut child = browser(name, profile, &format!("{base}?report=cold"))?;
        let report = next(reports);
        kill_tree(&mut child);
        let report = report?;
        let origin = value(&report, "origin");
        cold_first.push(origin + value(&report, "fcp") - start);
        cold_done.push(origin + value(&report, "load") - start);
        let _ = report.1.send(());
    }
    let mut row = Row {
        cold_first: median(&mut cold_first),
        cold_done: median(&mut cold_done),
        ..Row::default()
    };
    let mut child = browser(name, profile, &format!("{base}?report=full&warm={WARM}"))?;
    let first = next(reports);
    let measured = (|| {
        let first = first?;
        std::thread::sleep(SETTLE);
        for pid in tree(child.id()) {
            let (anon, rss) = memory(pid);
            row.total_anon += anon;
            row.total_rss += rss;
            if tab_process(pid) {
                row.tab_anon = row.tab_anon.max(anon);
            }
        }
        let _ = first.1.send(());
        let micro = next(reports)?;
        row.parse = value(&micro, "parse");
        row.restyle = value(&micro, "restyle");
        row.relayout = value(&micro, "relayout");
        row.scroll50 = value(&micro, "scroll50");
        row.scroll99 = value(&micro, "scroll99");
        let _ = micro.1.send(());
        let mut warm_first = Vec::new();
        let mut warm_done = Vec::new();
        for _ in 0..WARM {
            let report = next(reports)?;
            warm_first.push(value(&report, "fcp"));
            warm_done.push(value(&report, "load"));
            let _ = report.1.send(());
        }
        row.warm_first = median(&mut warm_first);
        row.warm_done = median(&mut warm_done);
        Ok::<(), std::io::Error>(())
    })();
    kill_tree(&mut child);
    measured?;
    Ok(row)
}

fn measure_bande(
    page: &str,
    port: u16,
    directory: &Path,
    rounds: usize,
) -> std::io::Result<Row> {
    let url = format!("http://127.0.0.1:{port}/{page}/index.html");
    let executable = std::env::current_exe()?;
    let mut cold_first = Vec::new();
    let mut cold_done = Vec::new();
    let mut row = Row::default();
    let mut warm_first = Vec::new();
    let mut warm_done = Vec::new();
    for round in 0..rounds {
        let last = round + 1 == rounds;
        let start = epoch_ms();
        let warm = if last { WARM.to_string() } else { "0".to_owned() };
        let output = Command::new(&executable)
            .args(["load", &url, &warm])
            .stdin(Stdio::null())
            .stderr(Stdio::inherit())
            .output()?;
        let text = String::from_utf8_lossy(&output.stdout);
        if !output.status.success() {
            return Err(std::io::Error::other(format!("load failed on {page}")));
        }
        for line in text.lines() {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let number = |index: usize| {
                fields
                    .get(index)
                    .and_then(|value| value.parse::<f64>().ok())
                    .unwrap_or(f64::NAN)
            };
            match fields.first() {
                Some(&"cold") => {
                    cold_first.push(number(1) - start);
                    cold_done.push(number(2) - start);
                }
                Some(&"warm") => {
                    warm_first.push(number(1));
                    warm_done.push(number(2));
                }
                Some(&"memory") if last => {
                    row.total_anon += number(2) as u64;
                    row.total_rss += number(3) as u64;
                    if fields.get(1).is_some_and(|name| name.contains("renderer")) {
                        row.tab_anon = number(2) as u64;
                    }
                }
                _ => {}
            }
        }
    }
    row.cold_first = median(&mut cold_first);
    row.cold_done = median(&mut cold_done);
    row.warm_first = median(&mut warm_first);
    row.warm_done = median(&mut warm_done);
    let stages = crate::stages::measure(&directory.join(page))?;
    row.parse = stages.parse;
    row.restyle = stages.restyle;
    row.relayout = stages.relayout;
    row.scroll50 = stages.scroll_p50;
    row.scroll99 = stages.scroll_p99;
    Ok(row)
}

fn machine() -> String {
    let host = std::fs::read_to_string("/etc/hostname").unwrap_or_default();
    let cpus = std::thread::available_parallelism().map_or(0, |count| count.get());
    format!("{} ({cpus} cpus)", host.trim())
}

fn version(program: &str) -> String {
    Command::new(program)
        .arg("--version")
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_default()
}

pub fn run(options: &Options) -> std::io::Result<()> {
    let root = PathBuf::from(&options.directory).canonicalize()?;
    let (port, reports) = server::start(root.clone())?;
    let profiles = std::env::temp_dir().join(format!("bench-profiles-{}", std::process::id()));
    println!(
        "machine: {}; {}; {}",
        machine(),
        version("firefox-esr"),
        version("chromium")
    );
    println!(
        "{:12} {:9} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>6} {:>6} {:>7} {:>7} {:>7}",
        "page",
        "browser",
        "coldFP",
        "coldEnd",
        "warmFP",
        "warmEnd",
        "parse",
        "restyle",
        "relay",
        "scr50",
        "scr99",
        "anonMB",
        "tabMB",
        "rssMB",
    );
    for page in &options.pages {
        for name in &options.browsers {
            let profile = profiles.join(name);
            if name == "firefox" {
                firefox_profile(&profile)?;
            } else {
                std::fs::create_dir_all(&profile)?;
            }
            let row = match name.as_str() {
                "bande" => measure_bande(page, port, &root, options.rounds),
                other => {
                    let warmup = measure_warmup(other, port, page, &reports, &profile);
                    warmup.and_then(|()| {
                        measure_browser(other, page, port, &reports, &profile, options.rounds)
                    })
                }
            };
            match row {
                Ok(row) => println!(
                    "{page:12} {name:9} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:6.2} {:6.2} {:7.1} {:7.1} {:7.1}",
                    row.cold_first,
                    row.cold_done,
                    row.warm_first,
                    row.warm_done,
                    row.parse,
                    row.restyle,
                    row.relayout,
                    row.scroll50,
                    row.scroll99,
                    row.total_anon as f64 / 1024.0,
                    row.tab_anon as f64 / 1024.0,
                    row.total_rss as f64 / 1024.0,
                ),
                Err(error) => println!("{page:12} {name:9} failed: {error}"),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&profiles);
    Ok(())
}

fn measure_warmup(
    name: &str,
    port: u16,
    page: &str,
    reports: &Receiver<Report>,
    profile: &Path,
) -> std::io::Result<()> {
    let url = format!("http://127.0.0.1:{port}/{page}/index.html?report=cold");
    let mut child = browser(name, profile, &url)?;
    let report = next(reports);
    kill_tree(&mut child);
    let report = report?;
    let _ = report.1.send(());
    Ok(())
}
