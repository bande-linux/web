use std::path::Path;
use std::time::{Duration, Instant};

use web_content::{Fonts, Page, Stages, Viewport};

use crate::{HEIGHT, Options, SCROLL_FRAMES, SCROLL_STEP, WIDTH, median, percentile};

#[derive(Default)]
pub struct Measured {
    pub elements: usize,
    pub parse: f64,
    pub stylesheets: f64,
    pub style: f64,
    pub boxes: f64,
    pub layout: f64,
    pub paint: f64,
    pub images: f64,
    pub raster: f64,
    pub total: f64,
    pub restyle: f64,
    pub relayout: f64,
    pub resize: f64,
    pub scroll_p50: f64,
    pub scroll_p99: f64,
    pub height: f32,
}

const ROUNDS: usize = 7;

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1e3
}

fn since(after: Stages, before: Stages) -> Stages {
    Stages {
        stylesheets: after.stylesheets.saturating_sub(before.stylesheets),
        style: after.style.saturating_sub(before.style),
        boxes: after.boxes.saturating_sub(before.boxes),
        layout: after.layout.saturating_sub(before.layout),
        paint: after.paint.saturating_sub(before.paint),
    }
}

fn fresh_fonts() -> Fonts {
    let mut fonts = Fonts::new();
    web_content::render_html("<p>a</p>", 100.0, &mut fonts);
    fonts
}

pub fn measure(directory: &Path) -> std::io::Result<Measured> {
    let source = std::fs::read(directory.join("index.html"))?;
    let viewport = Viewport {
        width: f32::from(WIDTH),
        height: f32::from(HEIGHT),
    };
    let mut rounds: Vec<Measured> = Vec::new();
    for _ in 0..ROUNDS {
        let mut fonts = fresh_fonts();
        let started = Instant::now();
        let document = web_content::parse(&source);
        let parse = started.elapsed();
        let mut page = Page::with_viewport(document, viewport, &mut fonts);
        page.load_stylesheets_from(directory, &mut fonts);
        let before_images = (Instant::now(), page.stages());
        page.load_fonts_from(directory, &mut fonts);
        page.load_images_from(directory, &mut fonts);
        let images_all = before_images.0.elapsed();
        let stages = page.stages();
        let images_stages = since(stages, before_images.1);
        let raster_start = Instant::now();
        let pixels = page.display_list.rasterize(0.0, WIDTH, HEIGHT);
        let raster = raster_start.elapsed();
        drop(pixels);
        let total = started.elapsed();
        let before_resize = page.stages();
        let resize_start = Instant::now();
        page.resize(f32::from(WIDTH) - 16.0, &mut fonts);
        let resize = resize_start.elapsed();
        let resized = since(page.stages(), before_resize);
        let in_images = images_stages.stylesheets
            + images_stages.style
            + images_stages.boxes
            + images_stages.layout
            + images_stages.paint;
        page.resize(f32::from(WIDTH), &mut fonts);
        let list = &page.display_list;
        let mut frames = Vec::with_capacity(SCROLL_FRAMES);
        let range = (list.height - f32::from(HEIGHT)).max(1.0);
        for frame in 0..SCROLL_FRAMES {
            let y = (frame as f32 * SCROLL_STEP) % range;
            let start = Instant::now();
            let pixels = list.rasterize(y, WIDTH, HEIGHT);
            frames.push(ms(start.elapsed()));
            drop(pixels);
        }
        rounds.push(Measured {
            elements: page.memory().elements,
            parse: ms(parse),
            stylesheets: ms(stages.stylesheets),
            style: ms(stages.style),
            boxes: ms(stages.boxes),
            layout: ms(stages.layout),
            paint: ms(stages.paint),
            images: ms(images_all.saturating_sub(in_images)),
            raster: ms(raster),
            total: ms(total),
            restyle: ms(resized.style),
            relayout: ms(resized.layout + resized.paint),
            resize: ms(resize),
            scroll_p50: percentile(&mut frames, 0.5),
            scroll_p99: percentile(&mut frames, 0.99),
            height: list.height,
        });
    }
    let mid = |field: fn(&Measured) -> f64, rounds: &[Measured]| {
        median(&mut rounds.iter().map(field).collect::<Vec<f64>>())
    };
    Ok(Measured {
        elements: rounds.first().map_or(0, |round| round.elements),
        height: rounds.first().map_or(0.0, |round| round.height),
        parse: mid(|m| m.parse, &rounds),
        stylesheets: mid(|m| m.stylesheets, &rounds),
        style: mid(|m| m.style, &rounds),
        boxes: mid(|m| m.boxes, &rounds),
        layout: mid(|m| m.layout, &rounds),
        paint: mid(|m| m.paint, &rounds),
        images: mid(|m| m.images, &rounds),
        raster: mid(|m| m.raster, &rounds),
        total: mid(|m| m.total, &rounds),
        restyle: mid(|m| m.restyle, &rounds),
        relayout: mid(|m| m.relayout, &rounds),
        resize: mid(|m| m.resize, &rounds),
        scroll_p50: mid(|m| m.scroll_p50, &rounds),
        scroll_p99: mid(|m| m.scroll_p99, &rounds),
    })
}

pub fn run(options: &Options) -> std::io::Result<()> {
    println!(
        "{:14} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>7} {:>8} {:>7} {:>8} {:>7}",
        "page (ms)",
        "elems",
        "parse",
        "css",
        "style",
        "boxes",
        "layout",
        "paint",
        "images",
        "raster",
        "total",
        "resize",
        "scrl p50",
        "p99"
    );
    for name in &options.pages {
        let m = measure(&options.directory.join(name))?;
        println!(
            "{name:14} {:7} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:7.1} {:8.1} {:7.1} {:8.2} {:7.2}",
            m.elements,
            m.parse,
            m.stylesheets,
            m.style,
            m.boxes,
            m.layout,
            m.paint,
            m.images,
            m.raster,
            m.total,
            m.resize,
            m.scroll_p50,
            m.scroll_p99,
        );
    }
    Ok(())
}
