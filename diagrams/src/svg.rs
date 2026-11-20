//! A minimal SVG writer. Output is inlined into the book, so it follows the
//! page theme: strokes and text use `currentColor`, accents use mdbook's CSS
//! variables, and there are no blank lines (which would end the HTML block
//! in Markdown).

use std::fmt::Write;

pub struct Svg {
    id: &'static str,
    width: f64,
    height: f64,
    body: String,
}

/// Stroke/fill classes defined in every diagram's `<style>`.
pub const FG: &str = "fg";
pub const ACCENT: &str = "ac";
pub const ACCENT2: &str = "ac2";
pub const MUTED: &str = "mu";

impl Svg {
    pub fn new(id: &'static str, width: f64, height: f64) -> Svg {
        Svg {
            id,
            width,
            height,
            body: String::new(),
        }
    }

    fn cls(&self, c: &str) -> String {
        format!("hv-{}-{c}", self.id)
    }

    pub fn line(&mut self, (x1, y1): (f64, f64), (x2, y2): (f64, f64), class: &str, dashed: bool) {
        let dash = if dashed {
            r#" stroke-dasharray="5 4""#
        } else {
            ""
        };
        let _ = write!(
            self.body,
            r#"<line x1="{x1:.1}" y1="{y1:.1}" x2="{x2:.1}" y2="{y2:.1}" class="{}"{dash}/>"#,
            self.cls(&format!("s{class}"))
        );
        self.body.push('\n');
    }

    pub fn arrow(&mut self, from: (f64, f64), to: (f64, f64), class: &str) {
        let _ = write!(
            self.body,
            r#"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" class="{}" marker-end="url(#hv-{}-arrow-{class})"/>"#,
            from.0,
            from.1,
            to.0,
            to.1,
            self.cls(&format!("s{class}")),
            self.id
        );
        self.body.push('\n');
    }

    pub fn polyline(&mut self, pts: &[(f64, f64)], class: &str, dashed: bool, width: f64) {
        if pts.len() < 2 {
            return;
        }
        // Drop points that lie (to 0.05 px) on the line between neighbours.
        let mut kept = vec![pts[0]];
        for w in pts.windows(3) {
            let (a, b, c) = (*kept.last().unwrap(), w[1], w[2]);
            let cross = (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0);
            let len = ((c.0 - a.0).powi(2) + (c.1 - a.1).powi(2)).sqrt().max(1e-9);
            if (cross / len).abs() > 0.05 {
                kept.push(b);
            }
        }
        kept.push(pts[pts.len() - 1]);
        let mut d = String::new();
        for (i, (x, y)) in kept.iter().enumerate() {
            let _ = write!(d, "{}{x:.1},{y:.1}", if i == 0 { "M" } else { " L" });
        }
        let dash = if dashed {
            r#" stroke-dasharray="5 4""#
        } else {
            ""
        };
        let _ = write!(
            self.body,
            r#"<path d="{d}" class="{}" fill="none" stroke-width="{width}"{dash}/>"#,
            self.cls(&format!("s{class}"))
        );
        self.body.push('\n');
    }

    pub fn circle(&mut self, (x, y): (f64, f64), r: f64, class: &str, filled: bool) {
        let c = if filled {
            format!("f{class}")
        } else {
            format!("s{class}")
        };
        let fill = if filled { "" } else { r#" fill="none""# };
        let _ = write!(
            self.body,
            r#"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" class="{}"{fill}/>"#,
            self.cls(&c)
        );
        self.body.push('\n');
    }

    pub fn rect(
        &mut self,
        (x, y): (f64, f64),
        (w, h): (f64, f64),
        class: &str,
        fill_class: Option<&str>,
        dashed: bool,
    ) {
        let fill = match fill_class {
            Some(f) => format!(
                r#" class="{} {}""#,
                self.cls(&format!("s{class}")),
                self.cls(&format!("bg{f}"))
            ),
            None => format!(r#" class="{}" fill="none""#, self.cls(&format!("s{class}"))),
        };
        let dash = if dashed {
            r#" stroke-dasharray="5 4""#
        } else {
            ""
        };
        let _ = write!(
            self.body,
            r#"<rect x="{x:.1}" y="{y:.1}" width="{w:.1}" height="{h:.1}" rx="6"{fill}{dash}/>"#
        );
        self.body.push('\n');
    }

    /// Text anchored at `(x, y)`; `anchor` is `start`, `middle` or `end`.
    pub fn text(&mut self, (x, y): (f64, f64), s: &str, anchor: &str, class: &str, size: f64) {
        let s = s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let _ = write!(
            self.body,
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}" font-size="{size}" class="{}">{s}</text>"#,
            self.cls(&format!("t{class}"))
        );
        self.body.push('\n');
    }

    pub fn mono(&mut self, (x, y): (f64, f64), s: &str, anchor: &str, class: &str, size: f64) {
        let s = s
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let _ = write!(
            self.body,
            r#"<text x="{x:.1}" y="{y:.1}" text-anchor="{anchor}" font-size="{size}" class="{} {}">{s}</text>"#,
            self.cls(&format!("t{class}")),
            self.cls("mono")
        );
        self.body.push('\n');
    }

    pub fn finish(self, title: &str) -> String {
        let id = self.id;
        let mut out = String::new();
        let _ = write!(
            out,
            r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 {} {}" class="hv-diagram" role="img" aria-labelledby="hv-{id}-title">"#,
            self.width, self.height
        );
        out.push('\n');
        let _ = write!(out, r#"<title id="hv-{id}-title">{title}</title>"#);
        out.push('\n');
        // Colours: fg follows the text colour; ac = links colour; ac2 = a
        // fixed orange that reads on light and dark themes; mu = muted.
        let _ = write!(
            out,
            "<style>.hv-{id}-sfg{{stroke:currentColor;stroke-width:1.5}}.hv-{id}-sac{{stroke:var(--links,#1f6feb);stroke-width:2}}.hv-{id}-sac2{{stroke:#e8590c;stroke-width:2}}.hv-{id}-smu{{stroke:var(--icons,#888);stroke-width:1}}.hv-{id}-ffg{{fill:currentColor}}.hv-{id}-fac{{fill:var(--links,#1f6feb)}}.hv-{id}-fac2{{fill:#e8590c}}.hv-{id}-fmu{{fill:var(--icons,#888)}}.hv-{id}-tfg{{fill:currentColor}}.hv-{id}-tac{{fill:var(--links,#1f6feb)}}.hv-{id}-tac2{{fill:#e8590c}}.hv-{id}-tmu{{fill:var(--icons,#888)}}.hv-{id}-bgfg{{fill:var(--quote-bg,rgba(127,127,127,.12))}}.hv-{id}-bgac{{fill:var(--quote-bg,rgba(127,127,127,.12))}}.hv-{id}-bgac2{{fill:rgba(232,89,12,.12)}}.hv-{id}-bgmu{{fill:rgba(127,127,127,.22)}}.hv-{id}-mono{{font-family:var(--mono-font,monospace)}}</style>"
        );
        out.push('\n');
        out.push_str("<defs>");
        for c in [FG, ACCENT, ACCENT2, MUTED] {
            let _ = write!(
                out,
                r#"<marker id="hv-{id}-arrow-{c}" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto-start-reverse"><path d="M0,0 L10,5 L0,10 z" class="hv-{id}-f{c}"/></marker>"#
            );
        }
        out.push_str("</defs>\n");
        out.push_str(&self.body);
        out.push_str("</svg>\n");
        out
    }
}
