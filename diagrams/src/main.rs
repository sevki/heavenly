//! `mdbook-heavenly`: an mdbook preprocessor that draws the book's diagrams.
//!
//! A chapter writes `{{#heavenly-diagram conics}}` and the preprocessor
//! replaces it with an inline SVG computed by the heavenly crates at book
//! build time. Figures that show physics (time scales, geodesy, orbits,
//! interpolation error) therefore always match the code that built the book.
//!
//! Configured in `book/book.toml`:
//!
//! ```toml
//! [preprocessor.heavenly]
//! command = "cargo run --quiet --locked -p heavenly-diagrams --"
//! ```
//!
//! Other modes, for working on figures:
//! * `heavenly-diagrams list` prints the diagram names;
//! * `heavenly-diagrams render <dir>` writes every diagram to `<dir>/<name>.svg`.

mod svg;

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::io::Read;
use std::path::PathBuf;
use std::process::ExitCode;

use heavenly_core::{Epoch, FrameId, StateVector, TimeScale, Vec3};
use heavenly_geo::{Ellipsoid, Geodetic};
use heavenly_orbit::{kepler, ConicElements, SampledTrajectory, TwoBodyLocator};
use serde_json::Value;
use svg::{Svg, ACCENT, ACCENT2, FG, MUTED};

type Figure = (&'static str, fn() -> String);

const FIGURES: [Figure; 14] = [
    ("crates", crates),
    ("frame-graph", frame_graph),
    ("transform", transform),
    ("time-scales", time_scales),
    ("geodetic", geodetic),
    ("orbital-elements", orbital_elements),
    ("conics", conics),
    ("sampled-trajectory", sampled_trajectory),
    ("daf", daf),
    ("earth-orientation", earth_orientation),
    ("registry", registry),
    ("light-time", light_time_figure),
    ("visibility", visibility),
    ("topology", topology),
];

const DIRECTIVE: &str = "{{#heavenly-diagram ";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // mdbook asks whether a renderer is supported; inline SVG works
        // anywhere HTML does, and is harmless elsewhere.
        Some("supports") => ExitCode::SUCCESS,
        Some("list") => {
            for (name, _) in FIGURES {
                println!("{name}");
            }
            ExitCode::SUCCESS
        }
        Some("render") => {
            let out = PathBuf::from(args.get(1).map_or("diagrams", String::as_str));
            std::fs::create_dir_all(&out).expect("create output directory");
            for (name, f) in FIGURES {
                let path = out.join(format!("{name}.svg"));
                std::fs::write(&path, f()).expect("write diagram");
                println!("wrote {}", path.display());
            }
            ExitCode::SUCCESS
        }
        None => match preprocess() {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("mdbook-heavenly: {e}");
                ExitCode::FAILURE
            }
        },
        Some(other) => {
            eprintln!(
                "mdbook-heavenly: unknown command {other:?} (expected supports, list or render)"
            );
            ExitCode::FAILURE
        }
    }
}

/// Reads `[context, book]` from stdin and writes the book back with every
/// directive expanded.
fn preprocess() -> Result<(), String> {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .map_err(|e| e.to_string())?;
    let parsed: Value =
        serde_json::from_str(&input).map_err(|e| format!("invalid input from mdbook: {e}"))?;
    let mut book = parsed
        .get(1)
        .cloned()
        .ok_or("expected [context, book] on stdin")?;
    // mdbook 0.5 calls the top-level list `items`; 0.4 called it `sections`.
    for key in ["items", "sections"] {
        if let Some(items) = book.get_mut(key) {
            expand_items(items)?;
        }
    }
    serde_json::to_writer(std::io::stdout(), &book).map_err(|e| e.to_string())
}

fn expand_items(items: &mut Value) -> Result<(), String> {
    for item in items.as_array_mut().into_iter().flatten() {
        if let Some(chapter) = item.get_mut("Chapter") {
            let path = chapter
                .get("path")
                .and_then(Value::as_str)
                .unwrap_or("?")
                .to_string();
            if let Some(Value::String(content)) = chapter.get_mut("content") {
                *content = expand(content).map_err(|e| format!("{path}: {e}"))?;
            }
            if let Some(sub) = chapter.get_mut("sub_items") {
                expand_items(sub)?;
            }
        }
    }
    Ok(())
}

/// Replaces each `{{#heavenly-diagram NAME}}` in `text` with that diagram.
/// As with mdbook's own directives, a leading backslash escapes one:
/// `\{{#heavenly-diagram NAME}}` is emitted literally, without the backslash.
fn expand(text: &str) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(DIRECTIVE) {
        if i > 0 && rest.as_bytes()[i - 1] == b'\\' {
            out.push_str(&rest[..i - 1]);
            out.push_str(DIRECTIVE);
            rest = &rest[i + DIRECTIVE.len()..];
            continue;
        }
        out.push_str(&rest[..i]);
        let after = &rest[i + DIRECTIVE.len()..];
        let end = after
            .find("}}")
            .ok_or("unterminated {{#heavenly-diagram …}}")?;
        let name = after[..end].trim();
        let (_, draw) = FIGURES.iter().find(|(n, _)| *n == name).ok_or_else(|| {
            let known: Vec<_> = FIGURES.iter().map(|(n, _)| *n).collect();
            format!("unknown diagram {name:?}; known: {}", known.join(", "))
        })?;
        out.push_str(draw().trim_end());
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    Ok(out)
}

/// A labelled box; returns its centre.
fn node(
    s: &mut Svg,
    (cx, cy): (f64, f64),
    w: f64,
    label: &str,
    sub: Option<&str>,
    class: &str,
) -> (f64, f64) {
    let h = if sub.is_some() { 42.0 } else { 28.0 };
    s.rect(
        (cx - w / 2.0, cy - h / 2.0),
        (w, h),
        class,
        Some(class),
        false,
    );
    match sub {
        Some(sub) => {
            s.mono((cx, cy - 3.0), label, "middle", FG, 13.0);
            s.text((cx, cy + 13.0), sub, "middle", MUTED, 10.5);
        }
        None => s.mono((cx, cy + 4.5), label, "middle", FG, 13.0),
    }
    (cx, cy)
}

/// Point on the boundary of a box of half-size (hw, hh) centred at `c`,
/// towards `to`.
fn edge(c: (f64, f64), (hw, hh): (f64, f64), to: (f64, f64)) -> (f64, f64) {
    let (dx, dy) = (to.0 - c.0, to.1 - c.1);
    let t = 1.0 / (dx.abs() / hw).max(dy.abs() / hh).max(1e-9);
    (c.0 + dx * t, c.1 + dy * t)
}

fn crates() -> String {
    let mut s = Svg::new("crates", 680.0, 330.0);
    s.text((340.0, 20.0), "your application", "middle", MUTED, 12.0);
    let top = node(
        &mut s,
        (340.0, 62.0),
        330.0,
        "heavenly",
        Some("heavenly = \"0.5\"  ·  crates.io/crates/heavenly"),
        ACCENT,
    );
    s.arrow((340.0, 28.0), (340.0, 39.0), MUTED);
    let modules = [
        ("::geo", "geodesy, Earth"),
        ("::spice", "ephemerides"),
        ("::orbit", "orbits"),
        ("::id", "identity"),
        ("::query", "visibility"),
        ("::topology", "failure domains"),
    ];
    let mut centres = Vec::new();
    for (i, (name, sub)) in modules.iter().enumerate() {
        let x = 60.0 + 112.0 * i as f64;
        centres.push(node(&mut s, (x, 165.0), 106.0, name, Some(sub), FG));
    }
    let core = node(
        &mut s,
        (340.0, 268.0),
        420.0,
        "core (crate root)",
        Some("Epoch · FrameId · Transform · StateVector · FrameGraph · Locator"),
        ACCENT2,
    );
    for c in &centres {
        s.arrow(
            edge(top, (165.0, 21.0), *c),
            edge(*c, (53.0, 21.0), top),
            MUTED,
        );
        s.arrow(
            edge(*c, (53.0, 21.0), core),
            edge(core, (210.0, 21.0), *c),
            FG,
        );
    }
    s.text((340.0, 318.0), "each module is a cargo feature (all on by default); arrows point to what a part depends on; everything is no_std", "middle", MUTED, 11.0);
    s.finish("The heavenly crate and its modules")
}

fn frame_graph() -> String {
    let mut s = Svg::new("frames", 680.0, 470.0);
    let w = 120.0;
    let hh = 14.0;
    let ssb = node(&mut s, (340.0, 40.0), w, "SSB_ICRF", None, ACCENT);
    let gcrf = node(&mut s, (170.0, 130.0), w, "GCRF", None, FG);
    let moon = node(&mut s, (510.0, 130.0), w, "MOON_ICRF", None, FG);
    let itrf = node(&mut s, (90.0, 230.0), w, "ITRF", None, FG);
    let body = node(
        &mut s,
        (300.0, 230.0),
        w + 20.0,
        "spacecraft body",
        None,
        FG,
    );
    let pa = node(&mut s, (440.0, 230.0), w, "MOON_PA", None, FG);
    let iau = node(&mut s, (600.0, 230.0), w, "MOON_IAU", None, FG);
    let london = node(&mut s, (90.0, 330.0), w, "london (ENU)", None, FG);
    let me = node(&mut s, (440.0, 330.0), w, "MOON_ME", None, FG);
    let shack = node(
        &mut s,
        (440.0, 420.0),
        w + 30.0,
        "shackleton (ENU)",
        None,
        FG,
    );
    let link =
        |s: &mut Svg, child: (f64, f64), parent: (f64, f64), cw: f64, label: &str, side: f64| {
            let a = edge(child, (cw / 2.0, hh), parent);
            let b = edge(parent, (w / 2.0, hh), child);
            s.arrow(a, b, FG);
            let m = ((a.0 + b.0) / 2.0 + side, (a.1 + b.1) / 2.0 + 4.0);
            s.text(
                m,
                label,
                if side < 0.0 { "end" } else { "start" },
                ACCENT,
                11.0,
            );
        };
    link(&mut s, gcrf, ssb, w, "spice: SPK", -8.0);
    link(&mut s, moon, ssb, w, "spice: SPK", 8.0);
    link(&mut s, itrf, gcrf, w, "geo: earth", -8.0);
    link(&mut s, body, gcrf, w + 20.0, "attitude (yours)", 8.0);
    link(&mut s, pa, moon, w, "spice: PCK", -8.0);
    link(&mut s, iau, moon, w, "geo: iau", 8.0);
    link(&mut s, london, itrf, w, "geo: ENU", -8.0);
    link(&mut s, me, pa, w, "spice: FK", 8.0);
    link(&mut s, shack, me, w + 30.0, "geo: ENU", 8.0);
    // Locators hang off frames.
    let loc = |s: &mut Svg, at: (f64, f64), frame: (f64, f64), label: &str, half: f64| {
        s.rect(
            (at.0 - half, at.1 - 12.0),
            (2.0 * half, 24.0),
            MUTED,
            None,
            true,
        );
        s.text((at.0, at.1 + 4.0), label, "middle", ACCENT2, 11.5);
        s.line(
            edge(at, (half, 12.0), frame),
            edge(frame, (w / 2.0, hh), at),
            ACCENT2,
            true,
        );
    };
    loc(&mut s, (90.0, 410.0), london, "FixedLocator: rack", 75.0);
    loc(
        &mut s,
        (205.0, 290.0),
        gcrf,
        "TwoBodyLocator / SampledTrajectory",
        105.0,
    );
    loc(&mut s, (590.0, 40.0), ssb, "SpkLocator: Mars", 75.0);
    s.text(
        (10.0, 462.0),
        "boxes: frames · arrows: child → parent edges, labelled with the crate that supplies them",
        "start",
        MUTED,
        11.0,
    );
    s.text(
        (10.0, 448.0),
        "dashed: locators (objects), which resolve into any connected frame",
        "start",
        ACCENT2,
        11.0,
    );
    s.finish("Frame graph for the Earth–Moon example")
}

fn transform() -> String {
    let mut s = Svg::new("transform", 680.0, 330.0);
    let ob = (110.0, 270.0);
    let oa = (360.0, 170.0);
    let p = (520.0, 70.0);
    // Frame B axes.
    s.arrow(ob, (ob.0 + 110.0, ob.1), FG);
    s.arrow(ob, (ob.0, ob.1 - 110.0), FG);
    s.text((ob.0 + 114.0, ob.1 + 4.0), "x_B", "start", FG, 13.0);
    s.text((ob.0 - 4.0, ob.1 - 114.0), "y_B", "end", FG, 13.0);
    s.text((ob.0 - 6.0, ob.1 + 16.0), "B", "end", FG, 14.0);
    // Frame A axes, rotated by 25°.
    let th: f64 = 25f64.to_radians();
    let ax = (oa.0 + 95.0 * th.cos(), oa.1 - 95.0 * th.sin());
    let ay = (oa.0 - 95.0 * th.sin(), oa.1 - 95.0 * th.cos());
    s.arrow(oa, ax, ACCENT);
    s.arrow(oa, ay, ACCENT);
    s.text((ax.0 + 4.0, ax.1 + 4.0), "x_A", "start", ACCENT, 13.0);
    s.text((ay.0 - 4.0, ay.1), "y_A", "end", ACCENT, 13.0);
    s.text((oa.0 + 30.0, oa.1 + 22.0), "A", "start", ACCENT, 14.0);
    // Rotation arc R between x_B direction and x_A at A's origin.
    let arc: Vec<_> = (0..=20)
        .map(|k| {
            let a = th * k as f64 / 20.0;
            (oa.0 + 55.0 * a.cos(), oa.1 - 55.0 * a.sin())
        })
        .collect();
    s.line(oa, (oa.0 + 70.0, oa.1), MUTED, true);
    s.polyline(&arc, MUTED, false, 1.2);
    s.text((oa.0 + 60.0, oa.1 - 8.0), "R", "start", MUTED, 13.0);
    // ω: circular arrow about A's origin.
    let w_arc: Vec<_> = (0..=30)
        .map(|k| {
            let a = 2.6 + 1.9 * k as f64 / 30.0;
            (oa.0 + 22.0 * a.cos(), oa.1 - 22.0 * a.sin())
        })
        .collect();
    s.polyline(&w_arc, ACCENT2, false, 1.6);
    s.arrow(w_arc[28], w_arc[30], ACCENT2);
    s.text((oa.0 - 30.0, oa.1 + 10.0), "ω", "end", ACCENT2, 14.0);
    // ṫ at A's origin.
    s.arrow(oa, (oa.0 + 20.0, oa.1 + 60.0), ACCENT2);
    s.text((oa.0 + 24.0, oa.1 + 66.0), "ṫ", "start", ACCENT2, 14.0);
    // Vectors.
    s.arrow(ob, (oa.0 - 4.0, oa.1 + 2.0), FG);
    s.text(
        ((ob.0 + oa.0) / 2.0 + 6.0, (ob.1 + oa.1) / 2.0 + 20.0),
        "t",
        "start",
        FG,
        14.0,
    );
    s.arrow(oa, p, ACCENT);
    s.text(
        ((oa.0 + p.0) / 2.0 - 10.0, (oa.1 + p.1) / 2.0 - 6.0),
        "p_A",
        "end",
        ACCENT,
        13.0,
    );
    s.arrow(ob, p, MUTED);
    s.text(
        ((ob.0 + p.0) / 2.0 - 10.0, (ob.1 + p.1) / 2.0 - 8.0),
        "p_B",
        "end",
        MUTED,
        13.0,
    );
    s.circle(p, 4.0, FG, true);
    s.text((p.0 + 8.0, p.1 - 4.0), "point", "start", FG, 12.0);
    s.text((420.0, 290.0), "p_B = R p_A + t", "start", FG, 14.0);
    s.text(
        (420.0, 312.0),
        "v_B = R v_A + ω × (R p_A) + ṫ",
        "start",
        FG,
        14.0,
    );
    s.text(
        (20.0, 22.0),
        "all four quantities (R, t, ṫ, ω) are expressed along B's axes",
        "start",
        MUTED,
        12.0,
    );
    s.finish("Transform from frame A to frame B")
}

/// Axes with ticks; returns a mapping closure from data to screen.
struct Plot {
    x0: f64,
    y0: f64,
    w: f64,
    h: f64,
    xr: (f64, f64),
    yr: (f64, f64),
}

impl Plot {
    fn map(&self, x: f64, y: f64) -> (f64, f64) {
        (
            self.x0 + (x - self.xr.0) / (self.xr.1 - self.xr.0) * self.w,
            self.y0 + self.h - (y - self.yr.0) / (self.yr.1 - self.yr.0) * self.h,
        )
    }

    fn axes(
        &self,
        s: &mut Svg,
        xticks: &[(f64, String)],
        yticks: &[(f64, String)],
        xlabel: &str,
        ylabel: &str,
    ) {
        let (x0, y1) = (self.x0, self.y0 + self.h);
        s.line((x0, y1), (x0 + self.w, y1), FG, false);
        s.line((x0, self.y0), (x0, y1), FG, false);
        for (x, l) in xticks {
            let (px, _) = self.map(*x, self.yr.0);
            s.line((px, y1), (px, y1 + 5.0), FG, false);
            s.text((px, y1 + 18.0), l, "middle", FG, 11.0);
        }
        for (y, l) in yticks {
            let (_, py) = self.map(self.xr.0, *y);
            s.line((x0 - 5.0, py), (x0, py), FG, false);
            s.line((x0, py), (x0 + self.w, py), MUTED, true);
            s.text((x0 - 8.0, py + 4.0), l, "end", FG, 11.0);
        }
        s.text(
            (x0 + self.w / 2.0, y1 + 34.0),
            xlabel,
            "middle",
            MUTED,
            11.5,
        );
        s.text((x0, self.y0 - 10.0), ylabel, "start", MUTED, 11.5);
    }
}

fn year_epoch(y: f64) -> Epoch {
    let whole = y.floor() as i32;
    let start = Epoch::from_gregorian(TimeScale::Tt, whole, 1, 1, 0, 0, 0, 0).unwrap();
    start
        .add_seconds((y - whole as f64) * 365.25 * 86400.0)
        .unwrap()
}

fn time_scales() -> String {
    let mut s = Svg::new("time", 680.0, 520.0);
    // Panel 1: TAI − UTC from the leap-second table, via Epoch.
    let p1 = Plot {
        x0: 70.0,
        y0: 40.0,
        w: 580.0,
        h: 170.0,
        xr: (1970.0, 2030.0),
        yr: (0.0, 75.0),
    };
    let xt: Vec<_> = (1970..=2030)
        .step_by(10)
        .map(|y| (y as f64, y.to_string()))
        .collect();
    let yt: Vec<_> = [0.0, 20.0, 40.0, 60.0]
        .iter()
        .map(|v| (*v, format!("{v} s")))
        .collect();
    p1.axes(&mut s, &xt, &yt, "year", "offset from UTC");
    let mut tai = Vec::new();
    let mut tt = Vec::new();
    let mut y = 1972.0;
    while y <= 2030.0 {
        let e = year_epoch(y);
        if let Ok(dat) = e.tai_minus_utc_seconds() {
            tai.push(p1.map(y, dat as f64));
            tt.push(p1.map(y, dat as f64 + 32.184));
        }
        y += 1.0 / 48.0;
    }
    s.polyline(&tai, ACCENT, false, 2.0);
    s.polyline(&tt, ACCENT2, false, 2.0);
    let (lx, ly) = p1.map(1984.0, 8.0);
    s.text((lx, ly), "TAI − UTC (leap seconds)", "start", ACCENT, 12.0);
    let (lx, ly) = p1.map(1984.0, 42.0);
    s.text(
        (lx, ly),
        "TT − UTC = TAI − UTC + 32.184 s",
        "start",
        ACCENT2,
        12.0,
    );
    let (lx, ly) = p1.map(2017.5, 30.0);
    s.text((lx, ly), "37 s since 2017", "middle", FG, 11.0);

    // Panel 2: TDB − TT over two years, from Epoch::tdb_minus_tt_seconds.
    let p2 = Plot {
        x0: 70.0,
        y0: 300.0,
        w: 580.0,
        h: 160.0,
        xr: (2026.0, 2028.0),
        yr: (-2.0, 2.0),
    };
    let xt2: Vec<_> = [2026.0, 2026.5, 2027.0, 2027.5, 2028.0]
        .iter()
        .map(|v| (*v, format!("{v}")))
        .collect();
    let yt2: Vec<_> = [-1.5, 0.0, 1.5]
        .iter()
        .map(|v| (*v, format!("{v} ms")))
        .collect();
    p2.axes(
        &mut s,
        &xt2,
        &yt2,
        "year",
        "TDB − TT (two-term model used by heavenly-core)",
    );
    let pts: Vec<_> = (0..=730)
        .map(|d| {
            let y = 2026.0 + d as f64 / 365.25;
            p2.map(y, year_epoch(y).tdb_minus_tt_seconds() * 1e3)
        })
        .collect();
    s.polyline(&pts, ACCENT, false, 2.0);
    s.text(
        (70.0, 515.0),
        "an Epoch stores TT; every other scale is computed from it at the boundary",
        "start",
        MUTED,
        11.0,
    );
    s.finish("Offsets between time scales")
}

fn geodetic() -> String {
    let mut s = Svg::new("geo", 680.0, 400.0);
    // Exaggerated flattening so the geodetic/geocentric difference is visible.
    let e = Ellipsoid::new(1.0, 0.3).unwrap();
    let scale = 230.0;
    let c = (300.0, 220.0);
    let to_px = |v: Vec3| (c.0 + v.x * scale, c.1 - v.z * scale);
    let outline: Vec<_> = (0..=180)
        .map(|k| {
            let lat = -FRAC_PI_2 + PI * k as f64 / 180.0;
            to_px(Geodetic::new(lat, 0.0, 0.0).unwrap().to_cartesian(&e))
        })
        .collect();
    s.polyline(&outline, FG, false, 1.8);
    let left: Vec<_> = outline.iter().map(|(x, y)| (2.0 * c.0 - x, *y)).collect();
    s.polyline(&left, MUTED, true, 1.0);
    s.line((c.0 - 250.0, c.1), (c.0 + 270.0, c.1), MUTED, true);
    s.line((c.0, c.1 + 180.0), (c.0, c.1 - 200.0), MUTED, true);
    s.text((c.0 + 274.0, c.1 + 4.0), "equator", "start", MUTED, 11.0);
    s.text((c.0 + 6.0, c.1 - 188.0), "pole", "start", MUTED, 11.0);
    s.circle(c, 3.0, FG, true);

    let lat = 45f64.to_radians();
    let h = 0.18;
    let surf = Geodetic::new(lat, 0.0, 0.0).unwrap().to_cartesian(&e);
    let site = Geodetic::new(lat, 0.0, h).unwrap().to_cartesian(&e);
    let (ps, pp) = (to_px(surf), to_px(site));
    // Normal through the surface point meets the equatorial plane off-centre.
    let n = Vec3::new(lat.cos(), 0.0, lat.sin());
    let back = surf - n * (surf.z / n.z);
    s.line(to_px(back), pp, ACCENT, false);
    s.line(c, pp, ACCENT2, false);
    // Angle arcs.
    let arc = |s: &mut Svg, at: (f64, f64), r: f64, a1: f64, class: &str| {
        let pts: Vec<_> = (0..=20)
            .map(|k| {
                let a = a1 * k as f64 / 20.0;
                (at.0 + r * a.cos(), at.1 - r * a.sin())
            })
            .collect();
        s.polyline(&pts, class, false, 1.5);
    };
    arc(&mut s, to_px(back), 40.0, lat, ACCENT);
    let psi = (site.z).atan2(site.x);
    arc(&mut s, c, 70.0, psi, ACCENT2);
    let bp = to_px(back);
    s.text(
        (bp.0 + 46.0, bp.1 - 8.0),
        "φ geodetic",
        "start",
        ACCENT,
        12.0,
    );
    s.text(
        (c.0 + 76.0, c.1 - 18.0),
        "ψ geocentric",
        "start",
        ACCENT2,
        12.0,
    );
    s.circle(ps, 3.5, FG, true);
    s.circle(pp, 4.0, ACCENT, true);
    s.text(
        ((ps.0 + pp.0) / 2.0 + 10.0, (ps.1 + pp.1) / 2.0 + 10.0),
        "h",
        "start",
        ACCENT,
        13.0,
    );
    // ENU axes at the site: up along the normal, north tangent, east out of the page.
    let up = (pp.0 + 60.0 * lat.cos(), pp.1 - 60.0 * lat.sin());
    let north = (pp.0 - 60.0 * lat.sin(), pp.1 - 60.0 * lat.cos());
    s.arrow(pp, up, FG);
    s.arrow(pp, north, FG);
    s.text((up.0 + 4.0, up.1), "up", "start", FG, 12.0);
    s.text((north.0 - 4.0, north.1), "north", "end", FG, 12.0);
    s.circle((pp.0 + 26.0, pp.1 + 16.0), 6.0, FG, false);
    s.circle((pp.0 + 26.0, pp.1 + 16.0), 1.6, FG, true);
    s.text(
        (pp.0 + 36.0, pp.1 + 21.0),
        "east (out of page)",
        "start",
        FG,
        12.0,
    );
    s.text(
        (20.0, 390.0),
        "meridian section; flattening exaggerated to 0.3 (WGS84 is 1/298.26)",
        "start",
        MUTED,
        11.0,
    );
    s.finish("Geodetic latitude, height and the local east-north-up frame")
}

/// Oblique projection of 3D points.
fn project(p: Vec3) -> (f64, f64) {
    let (az, el): (f64, f64) = (20f64.to_radians(), 22f64.to_radians());
    let right = Vec3::new(-az.sin(), az.cos(), 0.0);
    let up = Vec3::new(-el.sin() * az.cos(), -el.sin() * az.sin(), el.cos());
    let scale = 100.0;
    (330.0 + p.dot(right) * scale, 205.0 - p.dot(up) * scale)
}

fn orbital_elements() -> String {
    let mut s = Svg::new("elements", 680.0, 440.0);
    let (node_a, inc, argp) = (65f64.to_radians(), 40f64.to_radians(), 75f64.to_radians());
    let el = ConicElements {
        periapsis_m: 1.0,
        eccentricity: 0.45,
        inclination_rad: inc,
        ascending_node_rad: node_a,
        argument_of_periapsis_rad: argp,
        mean_anomaly_rad: 0.0,
        epoch: Epoch::J2000,
        gm_m3_s2: 1.0,
    };
    let period = el.period_s().unwrap();
    // Reference plane (circle of radius 1.6) and axes.
    let plane: Vec<_> = (0..=120)
        .map(|k| {
            let a = TAU * k as f64 / 120.0;
            project(Vec3::new(1.6 * a.cos(), 1.6 * a.sin(), 0.0))
        })
        .collect();
    s.polyline(&plane, MUTED, false, 1.0);
    s.arrow(project(Vec3::ZERO), project(Vec3::X * 1.9), FG);
    s.arrow(project(Vec3::ZERO), project(Vec3::Z * 1.3), FG);
    s.arrow(project(Vec3::ZERO), project(Vec3::Y * 1.8), MUTED);
    let px = project(Vec3::X * 1.95);
    s.text(
        (px.0, px.1 + 14.0),
        "x (reference direction)",
        "middle",
        FG,
        12.0,
    );
    let pz = project(Vec3::Z * 1.35);
    s.text((pz.0 + 6.0, pz.1), "z (reference pole)", "start", FG, 12.0);
    s.text(
        (20.0, 24.0),
        "reference plane: the xy-plane of the frame the elements are given in (e.g. ICRF equator)",
        "start",
        MUTED,
        11.5,
    );
    // Orbit, computed by heavenly-orbit; dashed below the reference plane.
    let mut above = Vec::new();
    let mut below = Vec::new();
    for k in 0..=360 {
        let (p, _) = el
            .state_at(Epoch::J2000.add_seconds(period * k as f64 / 360.0).unwrap())
            .unwrap();
        let pt = project(p);
        if p.z >= 0.0 {
            if !below.is_empty() {
                s.polyline(&below, ACCENT, true, 1.6);
                below.clear();
            }
            above.push(pt);
        } else {
            if !above.is_empty() {
                s.polyline(&above, ACCENT, false, 2.2);
                above.clear();
            }
            below.push(pt);
        }
    }
    s.polyline(&above, ACCENT, false, 2.2);
    s.polyline(&below, ACCENT, true, 1.6);
    // Node line.
    let nd = Vec3::new(node_a.cos(), node_a.sin(), 0.0);
    s.line(project(nd * -1.7), project(nd * 1.9), FG, true);
    let pn = project(nd * 1.95);
    s.text(
        (pn.0 + 4.0, pn.1 + 4.0),
        "ascending node",
        "start",
        FG,
        12.0,
    );
    // Ω arc in the reference plane.
    let om: Vec<_> = (0..=30)
        .map(|k| {
            let a = node_a * k as f64 / 30.0;
            project(Vec3::new(0.55 * a.cos(), 0.55 * a.sin(), 0.0))
        })
        .collect();
    s.polyline(&om, ACCENT2, false, 1.8);
    let pm = om[15];
    s.text((pm.0 - 6.0, pm.1 + 16.0), "Ω", "middle", ACCENT2, 15.0);
    // Orbit normal and inclination.
    let h = Vec3::new(
        inc.sin() * node_a.sin(),
        -inc.sin() * node_a.cos(),
        inc.cos(),
    );
    s.arrow(project(Vec3::ZERO), project(h * 1.15), ACCENT);
    let ph = project(h * 1.2);
    s.text(
        (ph.0 - 8.0, ph.1 + 2.0),
        "orbit normal",
        "end",
        ACCENT,
        12.0,
    );
    let ia: Vec<_> = (0..=20)
        .map(|k| {
            let a = inc * k as f64 / 20.0;
            let v = Vec3::Z * a.cos() + (h - Vec3::Z * inc.cos()).normalized().unwrap() * a.sin();
            project(v * 0.75)
        })
        .collect();
    s.polyline(&ia, ACCENT2, false, 1.8);
    let pi_ = ia[10];
    s.text((pi_.0 + 8.0, pi_.1 - 2.0), "i", "start", ACCENT2, 15.0);
    // ω from node to periapsis, in the orbit plane.
    let (peri, _) = el.state_at(Epoch::J2000).unwrap();
    let pu = peri.normalized().unwrap();
    let wa: Vec<_> = (0..=30)
        .map(|k| {
            let a = argp * k as f64 / 30.0;
            let v = nd * a.cos() + h.cross(nd) * a.sin();
            project(v * 0.42)
        })
        .collect();
    s.polyline(&wa, ACCENT2, false, 1.8);
    let pw = wa[15];
    s.text((pw.0 + 6.0, pw.1 - 4.0), "ω", "start", ACCENT2, 15.0);
    s.line(project(Vec3::ZERO), project(peri), FG, false);
    let pp = project(peri);
    s.circle(pp, 4.0, FG, true);
    s.text(
        (pp.0 + 8.0, pp.1 - 6.0),
        "periapsis (q from the focus)",
        "start",
        FG,
        12.0,
    );
    let _ = pu;
    // A body at true anomaly ν.
    let (body, _) = el
        .state_at(Epoch::J2000.add_seconds(period * 0.93).unwrap())
        .unwrap();
    let pb = project(body);
    s.line(project(Vec3::ZERO), pb, MUTED, true);
    s.circle(pb, 5.0, ACCENT, true);
    s.text(
        (pb.0 + 10.0, pb.1 + 18.0),
        "object at true anomaly ν",
        "start",
        ACCENT,
        12.0,
    );
    s.circle(project(Vec3::ZERO), 5.0, FG, true);
    s.text(
        (330.0, 425.0),
        "dashed: the part of the orbit below the reference plane",
        "middle",
        MUTED,
        11.0,
    );
    s.finish("Conic orbital elements")
}

fn conics() -> String {
    let mut s = Svg::new("conics", 680.0, 380.0);
    let (cx, cy, scale) = (560.0, 190.0, 46.0);
    let q = 1.0;
    let gm = 1.0;
    let ecc = [0.0, 0.5, 0.8, 1.0, 1.6];
    let classes = [FG, ACCENT, ACCENT, ACCENT2, ACCENT2];
    for (i, &e) in ecc.iter().enumerate() {
        let r0 = Vec3::new(q, 0.0, 0.0);
        let v0 = Vec3::new(0.0, (gm * (1.0 + e) / q).sqrt(), 0.0);
        let span = if e < 1.0 {
            let a: f64 = q / (1.0 - e);
            TAU * (a * a * a / gm).sqrt()
        } else {
            40.0
        };
        let n = 400;
        let mut pts: Vec<(f64, f64)> = Vec::new();
        for k in 0..=n {
            let t = if e < 1.0 {
                span * k as f64 / n as f64
            } else {
                span * (2.0 * k as f64 / n as f64 - 1.0)
            };
            let (r, _) = kepler::propagate(gm, r0, v0, t).unwrap();
            let p = (cx + r.x * scale, cy - r.y * scale);
            if p.0 > 10.0 && p.0 < 670.0 && p.1 > 40.0 && p.1 < 350.0 {
                pts.push(p);
            } else {
                // Leaving the frame: draw what we have and start a new piece.
                s.polyline(&pts, classes[i], e == 1.0, 2.0);
                pts.clear();
            }
        }
        s.polyline(&pts, classes[i], e == 1.0, 2.0);
    }
    s.circle((cx, cy), 6.0, FG, true);
    s.text(
        (cx - 12.0, cy + 26.0),
        "focus (central body)",
        "end",
        FG,
        11.5,
    );
    s.circle((cx + q * scale, cy), 3.5, FG, true);
    s.text((cx + q * scale + 6.0, cy - 6.0), "q", "start", FG, 13.0);
    let labels = [
        ((cx, cy - 52.0), "e = 0"),
        ((cx - 105.0, cy - 86.0), "e = 0.5"),
        ((cx - 400.0, 60.0), "e = 0.8"),
        ((cx - 190.0, 30.0), "e = 1 (parabola)"),
        ((cx + 30.0, 30.0), "e = 1.6"),
    ];
    for ((p, l), c) in labels.iter().zip(classes) {
        s.text(*p, l, "middle", c, 12.0);
    }
    s.text(
        (20.0, 370.0),
        "same periapsis q; every curve computed by kepler::propagate (one algorithm for all e)",
        "start",
        MUTED,
        11.0,
    );
    s.finish("Conics with a shared periapsis")
}

fn sampled_trajectory() -> String {
    let mut s = Svg::new("sampled", 680.0, 470.0);
    // Panel 1: coverage and gaps.
    let (x0, x1, y) = (60.0, 640.0, 70.0);
    s.text(
        (20.0, 22.0),
        "samples every 60 s, with three missing (max_gap_s = 90 s)",
        "start",
        MUTED,
        11.5,
    );
    let n = 14;
    let tick_x = |i: usize| x0 + 40.0 + (x1 - x0 - 80.0) * i as f64 / (n - 1) as f64;
    let missing = [6usize, 7, 8];
    s.rect(
        (x0 - 40.0, y - 22.0),
        (tick_x(0) - x0 + 40.0, 44.0),
        MUTED,
        Some(MUTED),
        false,
    );
    s.rect(
        (tick_x(n - 1), y - 22.0),
        (x1 + 40.0 - tick_x(n - 1), 44.0),
        MUTED,
        Some(MUTED),
        false,
    );
    s.rect(
        (tick_x(5), y - 22.0),
        (tick_x(9) - tick_x(5), 44.0),
        ACCENT2,
        None,
        true,
    );
    s.line((x0 - 40.0, y), (x1 + 40.0, y), FG, false);
    for i in 0..n {
        if missing.contains(&i) {
            s.circle((tick_x(i), y), 4.0, MUTED, false);
        } else {
            s.circle((tick_x(i), y), 4.5, ACCENT, true);
        }
    }
    s.text((x0 - 30.0, y + 40.0), "OutsideCoverage", "start", FG, 11.5);
    s.text((x1 + 30.0, y + 40.0), "OutsideCoverage", "end", FG, 11.5);
    s.text(
        ((tick_x(5) + tick_x(9)) / 2.0, y + 40.0),
        "DataGap { before, after }",
        "middle",
        ACCENT2,
        11.5,
    );
    s.text(
        ((tick_x(1) + tick_x(4)) / 2.0, y + 40.0),
        "interpolated",
        "middle",
        ACCENT,
        11.5,
    );
    s.text(
        ((tick_x(10) + tick_x(12)) / 2.0, y + 40.0),
        "interpolated",
        "middle",
        ACCENT,
        11.5,
    );

    // Panel 2: measured interpolation error for a LEO at 60 s spacing.
    let start = Epoch::J2000;
    let orbit = TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_778_137.0,
            eccentricity: 0.001,
            inclination_rad: 0.9,
            ascending_node_rad: 1.0,
            argument_of_periapsis_rad: 0.5,
            mean_anomaly_rad: 0.0,
            epoch: start,
            gm_m3_s2: heavenly_orbit::gm::EARTH,
        },
    };
    let step = 60.0;
    let samples: Vec<StateVector> = (0..12)
        .map(|i| {
            orbit
                .state(start.add_seconds(i as f64 * step).unwrap())
                .unwrap()
        })
        .collect();
    let traj = SampledTrajectory::new(samples, 90.0).unwrap();
    let omega = orbit.elements.mean_motion_rad_s();
    let bound = step.powi(4) / 384.0 * 6.778e6 * omega.powi(4);
    let p = Plot {
        x0: 70.0,
        y0: 170.0,
        w: 570.0,
        h: 230.0,
        xr: (0.0, 600.0),
        yr: (0.0, 0.5),
    };
    let xt: Vec<_> = (0..=10)
        .map(|k| (k as f64 * 60.0, format!("{}", k * 60)))
        .collect();
    let yt: Vec<_> = [0.0, 0.1, 0.2, 0.3, 0.4]
        .iter()
        .map(|v| (*v, format!("{v} m")))
        .collect();
    p.axes(
        &mut s,
        &xt,
        &yt,
        "seconds after the first sample (samples at every tick)",
        "|interpolated − true position|, LEO, cubic Hermite",
    );
    let pts: Vec<_> = (0..=1200)
        .map(|k| {
            let t = k as f64 * 0.5;
            let e = start.add_seconds(t).unwrap();
            let d = (traj.state(e).unwrap().position_m - orbit.state(e).unwrap().position_m).norm();
            p.map(t, d)
        })
        .collect();
    s.polyline(&pts, ACCENT, false, 1.8);
    let (bx0, by) = p.map(0.0, bound);
    let (bx1, _) = p.map(600.0, bound);
    s.line((bx0, by), (bx1, by), ACCENT2, true);
    s.text(
        (bx1, by - 16.0),
        &format!("bound h⁴/384·rω⁴ = {bound:.2} m"),
        "end",
        ACCENT2,
        11.5,
    );
    s.text(
        (20.0, 462.0),
        "panel 2 is measured: SampledTrajectory against the analytic TwoBodyLocator orbit",
        "start",
        MUTED,
        11.0,
    );
    s.finish("Sampled trajectories: coverage, gaps and interpolation error")
}

fn daf() -> String {
    let mut s = Svg::new("daf", 680.0, 400.0);
    s.text(
        (20.0, 22.0),
        "a DAF file is a sequence of 1024-byte records (SPK and binary PCK share the format)",
        "start",
        MUTED,
        11.5,
    );
    let recs = [
        ("file record", "ND, NI, FWARD, format"),
        ("comments", "text"),
        ("summary", "next, prev, n, summaries"),
        ("names", "segment names"),
        ("data …", "f64 words"),
    ];
    let (y, h) = (40.0, 52.0);
    let mut x = 20.0;
    let widths = [130.0, 90.0, 150.0, 100.0, 170.0];
    let mut centres = Vec::new();
    for ((label, sub), w) in recs.iter().zip(widths) {
        let class = if *label == "summary" { ACCENT } else { FG };
        s.rect((x, y), (w - 6.0, h), class, Some(class), false);
        s.mono((x + (w - 6.0) / 2.0, y + 22.0), label, "middle", FG, 12.5);
        s.text((x + (w - 6.0) / 2.0, y + 40.0), sub, "middle", MUTED, 10.5);
        centres.push(x + (w - 6.0) / 2.0);
        x += w;
    }
    // Segment layout.
    let sy = 170.0;
    s.text(
        (20.0, sy - 14.0),
        "one segment (Chebyshev type 2): fixed-size records, then a 4-word directory",
        "start",
        MUTED,
        11.5,
    );
    let parts = [
        ("record 1", 95.0),
        ("record 2", 95.0),
        ("…", 50.0),
        ("record N", 95.0),
        ("INIT", 70.0),
        ("INTLEN", 75.0),
        ("RSIZE", 70.0),
        ("N", 50.0),
    ];
    let mut x = 20.0;
    let mut first = 0.0;
    let mut last = 0.0;
    for (i, (label, w)) in parts.iter().enumerate() {
        let class = if i >= 4 { ACCENT2 } else { FG };
        s.rect((x, sy), (w - 4.0, 36.0), class, Some(class), false);
        s.mono((x + (w - 4.0) / 2.0, sy + 23.0), label, "middle", FG, 12.0);
        if i == 0 {
            first = x;
        }
        last = x + w - 4.0;
        x += w;
    }
    // Summary points at the segment's start and end addresses.
    s.arrow((centres[2] - 20.0, y + h), (first + 4.0, sy - 2.0), ACCENT);
    s.arrow((centres[2] + 20.0, y + h), (last - 4.0, sy - 2.0), ACCENT);
    s.text(
        (centres[2], y + h + 16.0),
        "start / end word address",
        "middle",
        ACCENT,
        11.0,
    );
    // Record layout.
    let ry = 290.0;
    s.text(
        (20.0, ry - 14.0),
        "one record covers [MID − RADIUS, MID + RADIUS]; x, y, z each have degree + 1 coefficients",
        "start",
        MUTED,
        11.5,
    );
    let rparts = [
        ("MID", 70.0),
        ("RADIUS", 80.0),
        ("x: c₀ … cₙ", 160.0),
        ("y: c₀ … cₙ", 160.0),
        ("z: c₀ … cₙ", 160.0),
    ];
    let mut x = 20.0;
    for (label, w) in rparts {
        s.rect((x, ry), (w - 4.0, 36.0), FG, Some(FG), false);
        s.mono((x + (w - 4.0) / 2.0, ry + 23.0), label, "middle", FG, 12.0);
        x += w;
    }
    s.arrow((70.0, sy + 38.0), (60.0, ry - 2.0), MUTED);
    s.text((20.0, 360.0), "record index = ⌊(ET − INIT) / INTLEN⌋;  position = Σ cₖ Tₖ((ET − MID) / RADIUS);  velocity by differentiating the series", "start", FG, 11.5);
    s.text(
        (20.0, 384.0),
        "type 3 stores six series (position and velocity); PCK type 2 stores three Euler angles",
        "start",
        MUTED,
        11.0,
    );
    s.finish("DAF and SPK segment layout")
}

fn earth_orientation() -> String {
    let mut s = Svg::new("eo", 680.0, 210.0);
    let steps = [
        ("GCRS", "celestial (GCRF)"),
        ("CIRS", "bias-precession-nutation (X, Y, s)"),
        ("TIRS", "Earth Rotation Angle (UT1)"),
        ("ITRS", "polar motion (xp, yp, s′)"),
    ];
    let y = 70.0;
    let xs = [70.0, 250.0, 430.0, 610.0];
    for (i, ((name, _), x)) in steps.iter().zip(xs).enumerate() {
        node(
            &mut s,
            (x, y),
            110.0,
            name,
            None,
            if i == 0 || i == 3 { ACCENT } else { FG },
        );
    }
    for i in 0..3 {
        s.arrow((xs[i] + 57.0, y), (xs[i + 1] - 57.0, y), FG);
        s.text(
            ((xs[i] + xs[i + 1]) / 2.0, y + 40.0),
            steps[i + 1].1,
            "middle",
            ACCENT2,
            11.5,
        );
    }
    s.text((70.0, y - 26.0), "FrameId::GCRF", "middle", MUTED, 11.0);
    s.text((610.0, y - 26.0), "FrameId::ITRF", "middle", MUTED, 11.0);
    s.text((20.0, 160.0), "M = W(xp, yp, s′) · R₃(ERA) · Q(X, Y, s)      (IERS 2010 CIO-based procedure, as SOFA c2t06a)", "start", FG, 12.0);
    s.text((20.0, 186.0), "precession in full (IAU 2006), nutation truncated to 20 terms: 7.6 mas from SOFA over 1980–2050", "start", MUTED, 11.0);
    s.finish("Earth orientation chain")
}

fn registry() -> String {
    let mut s = Svg::new("registry", 680.0, 470.0);
    s.text(
        (110.0, 20.0),
        "aliases (mutable names)",
        "middle",
        MUTED,
        11.5,
    );
    s.text(
        (310.0, 20.0),
        "LocationId (identity)",
        "middle",
        MUTED,
        11.5,
    );
    s.text(
        (560.0, 20.0),
        "entities and their frames",
        "middle",
        MUTED,
        11.5,
    );
    let ex = 560.0;
    let ew = 170.0;
    let frame = |s: &mut Svg, y: f64, name: &str| node(s, (ex, y), 110.0, name, None, ACCENT);
    let entity =
        |s: &mut Svg, y: f64, name: &str, mount: &str| node(s, (ex, y), ew, name, Some(mount), FG);
    let up = |s: &mut Svg, from_y: f64, to_y: f64, label: &str| {
        s.arrow((ex, from_y - 21.0), (ex, to_y + 21.0), FG);
        s.text(
            (ex - 8.0, (from_y + to_y) / 2.0 + 4.0),
            label,
            "end",
            ACCENT,
            10.5,
        );
    };
    // Ground tree.
    frame(&mut s, 50.0, "ITRF");
    entity(&mut s, 115.0, "London LHR1", "Kind::Facility");
    entity(&mut s, 180.0, "Hall 2", "Kind::Room");
    entity(&mut s, 245.0, "Rack 7", "Kind::Rack");
    s.arrow((ex, 115.0 - 21.0), (ex, 50.0 + 14.0), FG);
    s.text(
        (ex - 8.0, 83.0),
        "Mount::Geodetic (ENU)",
        "end",
        ACCENT,
        10.5,
    );
    up(&mut s, 180.0, 115.0, "Mount::Fixed");
    up(&mut s, 245.0, 180.0, "Mount::Fixed");
    // Space tree.
    frame(&mut s, 295.0, "GCRF");
    entity(&mut s, 360.0, "sat-42", "Kind::Spacecraft");
    entity(&mut s, 425.0, "Rack 1", "Kind::Rack (in Module 2)");
    s.arrow((ex, 360.0 - 21.0), (ex, 295.0 + 14.0), FG);
    s.text(
        (ex - 8.0, 328.0),
        "Mount::Trajectory (LVLH)",
        "end",
        ACCENT,
        10.5,
    );
    up(&mut s, 425.0, 360.0, "Mount::Fixed ×2");
    // Aliases → IDs → entities.
    let rows = [
        (115.0, "dc:lhr:01", "6f1c0b4e…"),
        (245.0, "dc:lhr:01/hall-2/rack-07", "9a8b7c6d…"),
        (360.0, "sat:example:42", "3c2b1a09…"),
        (425.0, "sat:example:42/module-2/rack-1", "7c6b5a4f…"),
    ];
    for (y, alias, id) in rows {
        s.rect((8.0, y - 13.0), (204.0, 26.0), MUTED, None, true);
        s.mono((110.0, y + 4.0), alias, "middle", FG, 10.5);
        s.rect(
            (250.0, y - 13.0),
            (120.0, 26.0),
            ACCENT2,
            Some(ACCENT2),
            false,
        );
        s.mono((310.0, y + 4.0), id, "middle", FG, 11.0);
        s.arrow((213.0, y), (248.0, y), MUTED);
        s.arrow((371.0, y), (ex - ew / 2.0 - 2.0, y), ACCENT2);
    }
    s.text((20.0, 462.0), "rename / repoint changes only the left column; register_frames() adds one frame-graph edge per entity", "start", MUTED, 11.0);
    s.finish("Aliases, identities and entity frames in a Registry")
}

fn light_time_figure() -> String {
    let mut s = Svg::new("lighttime", 680.0, 300.0);
    // Observer fixed at t; target moving along a path.
    let obs = (90.0, 230.0);
    s.circle(obs, 6.0, FG, true);
    s.text((obs.0, obs.1 + 22.0), "observer at t", "middle", FG, 12.0);
    let path: Vec<_> = (0..=40)
        .map(|k| {
            let x = 330.0 + 8.0 * k as f64;
            (x, 70.0 + 0.0009 * (x - 330.0).powi(2))
        })
        .collect();
    s.polyline(&path, MUTED, true, 1.4);
    s.text((640.0, 60.0), "target's path", "end", MUTED, 11.5);
    let then = path[10];
    let now = path[22];
    let later = path[34];
    s.circle(then, 5.5, ACCENT, true);
    s.circle(now, 5.0, MUTED, false);
    s.circle(later, 5.5, ACCENT2, true);
    s.text(
        (then.0 - 4.0, then.1 - 12.0),
        "t − τ",
        "middle",
        ACCENT,
        12.0,
    );
    s.text((now.0, now.1 - 12.0), "t", "middle", MUTED, 12.0);
    s.text(
        (later.0 + 4.0, later.1 - 12.0),
        "t + τ",
        "middle",
        ACCENT2,
        12.0,
    );
    s.arrow(then, (obs.0 + 6.0, obs.1 - 4.0), ACCENT);
    s.text(
        ((then.0 + obs.0) / 2.0 - 30.0, (then.1 + obs.1) / 2.0 - 8.0),
        "Reception (CN): what arrives at t left at t − τ",
        "middle",
        ACCENT,
        11.5,
    );
    s.arrow((obs.0 + 7.0, obs.1 + 1.0), later, ACCENT2);
    s.text(
        (
            (later.0 + obs.0) / 2.0 + 40.0,
            (later.1 + obs.1) / 2.0 + 26.0,
        ),
        "Transmission (XCN): sent at t, arrives at t + τ",
        "middle",
        ACCENT2,
        11.5,
    );
    s.line(obs, now, MUTED, true);
    s.text((20.0, 290.0), "τ = |r_target(t ∓ τ) − r_observer(t)| / c, iterated to 1 ps; distance at a common epoch (dashed) is not signal delay", "start", MUTED, 11.0);
    s.finish("Light time on reception and on transmission")
}

fn visibility() -> String {
    use heavenly_core::{FixedLocator, FrameGraph, Namespace};
    use heavenly_geo::earth;
    use heavenly_query::{find_windows, look_angles};
    let mut s = Svg::new("visibility", 680.0, 330.0);
    let start = Epoch::from_gregorian(TimeScale::Utc, 2026, 11, 20, 9, 0, 0, 0).unwrap();
    let mut graph = FrameGraph::new();
    earth::register(&mut graph, earth::EarthOrientation::ZERO).unwrap();
    let site = FrameId::custom(Namespace(3), 1).unwrap();
    let g = Geodetic::from_degrees(51.5072, -0.1276, 35.0).unwrap();
    graph
        .add_fixed(site, FrameId::ITRF, g.enu_to_body_fixed(&Ellipsoid::WGS84))
        .unwrap();
    let _ = FixedLocator {
        frame: site,
        position_m: Vec3::ZERO,
    };
    let sat = TwoBodyLocator {
        frame: FrameId::GCRF,
        elements: ConicElements {
            periapsis_m: 6_798_137.0,
            eccentricity: 0.0005,
            inclination_rad: 51.64f64.to_radians(),
            ascending_node_rad: 3.0,
            argument_of_periapsis_rad: 0.0,
            mean_anomaly_rad: 0.0,
            epoch: Epoch::from_gregorian(TimeScale::Utc, 2026, 11, 19, 17, 0, 0, 0).unwrap(),
            gm_m3_s2: heavenly_orbit::gm::EARTH,
        },
    };
    let hours = 6.0;
    let el = |t: Epoch| -> f64 {
        look_angles::<_, _, heavenly_core::Error>(site, &sat, t, &graph)
            .unwrap()
            .elevation_rad
            .to_degrees()
    };
    let p = Plot {
        x0: 60.0,
        y0: 30.0,
        w: 590.0,
        h: 230.0,
        xr: (0.0, hours),
        yr: (-90.0, 90.0),
    };
    let mask = 10.0;
    let windows = find_windows::<heavenly_core::Error>(
        start,
        start.add_seconds(hours * 3600.0).unwrap(),
        30.0,
        1e-3,
        |t| Ok(el(t) - mask),
    )
    .unwrap();
    for w in &windows {
        let a = p.map(w.start.seconds_since(start) / 3600.0, 90.0);
        let b = p.map(w.end.seconds_since(start) / 3600.0, -90.0);
        s.rect(
            (a.0, a.1),
            ((b.0 - a.0).max(2.0), b.1 - a.1),
            ACCENT2,
            Some(ACCENT2),
            false,
        );
    }
    let xt: Vec<_> = (0..=6)
        .map(|h| (h as f64, format!("{:02}:00", 9 + h)))
        .collect();
    let yt: Vec<_> = [-90.0, -45.0, 0.0, 45.0, 90.0]
        .iter()
        .map(|v| (*v, format!("{v}°")))
        .collect();
    p.axes(
        &mut s,
        &xt,
        &yt,
        "UTC, 20 November 2026",
        "elevation of an ISS-like orbit seen from London",
    );
    let pts: Vec<_> = (0..=2160)
        .map(|k| {
            let h = hours * k as f64 / 2160.0;
            p.map(h, el(start.add_seconds(h * 3600.0).unwrap()))
        })
        .collect();
    s.polyline(&pts, ACCENT, false, 1.6);
    let (mx0, my) = p.map(0.0, mask);
    let (mx1, _) = p.map(hours, mask);
    s.line((mx0, my), (mx1, my), ACCENT2, true);
    s.text((mx1, my - 6.0), "10° mask", "end", ACCENT2, 11.0);
    s.text((20.0, 322.0), &format!("{} windows found by find_windows (30 s steps, bisected to 1 ms); computed by heavenly-query", windows.len()), "start", MUTED, 11.0);
    s.finish("Visibility windows")
}

fn topology() -> String {
    let mut s = Svg::new("topology", 680.0, 330.0);
    s.text(
        (20.0, 20.0),
        "domains (policy) and their dependencies; locations are assigned to domains",
        "start",
        MUTED,
        11.5,
    );
    let w = 150.0;
    let grid = node(
        &mut s,
        (250.0, 70.0),
        w,
        "grid:uk-south",
        Some("DomainKind::Power"),
        ACCENT2,
    );
    let gb = node(
        &mut s,
        (80.0, 70.0),
        120.0,
        "law:gb",
        Some("Jurisdiction"),
        FG,
    );
    let ost = node(
        &mut s,
        (600.0, 70.0),
        120.0,
        "law:ost",
        Some("Jurisdiction"),
        FG,
    );
    let za = node(&mut s, (90.0, 170.0), w, "zone:lhr-a", Some("Zone"), FG);
    let zb = node(&mut s, (250.0, 170.0), w, "zone:lhr-b", Some("Zone"), FG);
    let net = node(
        &mut s,
        (420.0, 170.0),
        w,
        "net:ground-uk",
        Some("Network"),
        FG,
    );
    let con = node(
        &mut s,
        (590.0, 170.0),
        w - 10.0,
        "constellation",
        Some("Constellation"),
        FG,
    );
    let dep = |s: &mut Svg, a: (f64, f64), b: (f64, f64), hw: f64| {
        s.arrow(edge(a, (hw, 21.0), b), edge(b, (hw, 21.0), a), FG);
    };
    dep(&mut s, za, grid, 75.0);
    dep(&mut s, zb, grid, 75.0);
    dep(&mut s, net, grid, 75.0);
    dep(&mut s, con, net, 70.0);
    dep(&mut s, za, gb, 60.0);
    dep(&mut s, zb, gb, 60.0);
    dep(&mut s, con, ost, 60.0);
    let loc = |s: &mut Svg, at: (f64, f64), label: &str, dom: (f64, f64)| {
        s.rect((at.0 - 70.0, at.1 - 13.0), (140.0, 26.0), MUTED, None, true);
        s.text((at.0, at.1 + 4.0), label, "middle", ACCENT, 11.5);
        s.line((at.0, at.1 - 13.0), (dom.0, dom.1 + 21.0), ACCENT, true);
    };
    loc(&mut s, (90.0, 260.0), "rack A (London)", za);
    loc(&mut s, (250.0, 260.0), "rack B (London)", zb);
    loc(&mut s, (590.0, 260.0), "sat-42 rack (orbit)", con);
    s.text((20.0, 305.0), "arrows: depends on · dashed: assigned to · shared_failure_domains(rack A, sat-42) = { grid:uk-south }", "start", FG, 11.5);
    s.text((20.0, 322.0), "7 000 km apart, one grid in common; blast_radius(\"grid:uk-south\") = { rack A, rack B, sat-42 }", "start", MUTED, 11.0);
    s.finish("Failure domains and jurisdictions")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn expands_directives_and_rejects_unknown_names() {
        let md = "before\n{{#heavenly-diagram conics}}\nafter";
        let out = expand(md).unwrap();
        assert!(out.starts_with("before\n<svg") && out.ends_with("</svg>\nafter"));
        // Inline SVG must not contain blank lines, which would end the HTML block.
        assert!(!out.contains("\n\n"));
        assert!(expand("{{#heavenly-diagram nope}}")
            .unwrap_err()
            .contains("unknown diagram"));
        assert!(expand("{{#heavenly-diagram conics").is_err());
        assert_eq!(expand("no directives").unwrap(), "no directives");
        assert_eq!(
            expand(r"see `\{{#heavenly-diagram conics}}`").unwrap(),
            "see `{{#heavenly-diagram conics}}`"
        );
    }

    #[test]
    fn every_figure_renders() {
        for (name, f) in FIGURES {
            let svg = f();
            assert!(
                svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"),
                "{name}"
            );
            assert!(!svg.contains("NaN"), "{name} contains NaN coordinates");
        }
    }

    #[test]
    fn walks_nested_chapters() {
        let mut items = serde_json::json!([
            {"Chapter": {"path": "a.md", "content": "{{#heavenly-diagram daf}}", "sub_items": [
                {"Chapter": {"path": "b.md", "content": "x {{#heavenly-diagram crates}} y", "sub_items": []}}
            ]}},
            "Separator"
        ]);
        expand_items(&mut items).unwrap();
        let b = items[0]["Chapter"]["sub_items"][0]["Chapter"]["content"]
            .as_str()
            .unwrap();
        assert!(b.starts_with("x <svg") && b.ends_with("</svg> y"));
    }
}
