//! Generates the book's diagrams: `cargo run -p heavenly-diagrams`.
//!
//! Figures that show physics are computed with the heavenly crates
//! themselves, so they cannot drift from the code. CI regenerates them and
//! fails if the committed SVGs differ.

mod svg;

use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::path::PathBuf;

use heavenly_core::{Epoch, FrameId, StateVector, TimeScale, Vec3};
use heavenly_geo::{Ellipsoid, Geodetic};
use heavenly_orbit::{kepler, ConicElements, SampledTrajectory, TwoBodyLocator};
use svg::{Svg, ACCENT, ACCENT2, FG, MUTED};

fn main() {
    let out = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../book/src/diagrams"));
    std::fs::create_dir_all(&out).unwrap();
    type Figure = (&'static str, fn() -> String);
    let figures: [Figure; 10] = [
        ("crates.svg", crates),
        ("frame-graph.svg", frame_graph),
        ("transform.svg", transform),
        ("time-scales.svg", time_scales),
        ("geodetic.svg", geodetic),
        ("orbital-elements.svg", orbital_elements),
        ("conics.svg", conics),
        ("sampled-trajectory.svg", sampled_trajectory),
        ("daf.svg", daf),
        ("earth-orientation.svg", earth_orientation),
    ];
    for (name, f) in figures {
        std::fs::write(out.join(name), f()).unwrap();
        println!("wrote {}", out.join(name).display());
    }
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
    let mut s = Svg::new("crates", 680.0, 300.0);
    s.text(
        (340.0, 24.0),
        "your application: facilities, spacecraft, schedulers",
        "middle",
        MUTED,
        12.0,
    );
    let core = node(
        &mut s,
        (340.0, 250.0),
        340.0,
        "heavenly-core",
        Some("Epoch · FrameId · Transform · FrameGraph · Locator"),
        ACCENT,
    );
    let geo = node(
        &mut s,
        (115.0, 140.0),
        214.0,
        "heavenly-geo",
        Some("ellipsoids · Earth/Moon/Mars orientation"),
        FG,
    );
    let spice = node(
        &mut s,
        (340.0, 140.0),
        214.0,
        "heavenly-spice",
        Some("SPK/PCK kernels · ephemerides"),
        FG,
    );
    let orbit = node(
        &mut s,
        (565.0, 140.0),
        214.0,
        "heavenly-orbit",
        Some("two-body · elements · trajectories"),
        FG,
    );
    for c in [geo, spice, orbit] {
        let a = edge(c, (107.0, 21.0), core);
        let b = edge(core, (170.0, 21.0), c);
        s.arrow(a, b, FG);
        s.arrow((c.0, 44.0), (c.0, c.1 - 23.0), MUTED);
    }
    s.text(
        (340.0, 290.0),
        "arrows point from a crate to what it depends on; all are no_std",
        "middle",
        MUTED,
        11.0,
    );
    s.finish("Crate architecture")
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
