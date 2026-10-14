//! Just enough SVG to draw stroke icons (Lucide's 24×24 set): `path`, `rect`, `circle`.
//! Shapes become polylines once; drawing is then a scale and a stroke.

use eframe::egui::{Pos2, Vec2, pos2, vec2};
use std::f32::consts::{PI, TAU};

#[derive(Debug, Clone, PartialEq)]
pub enum Part {
    Line { points: Vec<Pos2>, closed: bool },
    Dot { centre: Pos2, radius: f32 },
}

/// Parse the drawable elements of an icon file.
pub fn parse(svg: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    for element in svg.split('<').skip(1) {
        let name = element.split(|c: char| c.is_whitespace() || c == '/' || c == '>').next().unwrap_or("");
        let num = |key: &str| attr(element, key).and_then(|v| v.parse::<f32>().ok());
        match name {
            "path" => {
                if let Some(d) = attr(element, "d") {
                    parts.extend(path(d));
                }
            }
            "rect" => {
                let (x, y) = (num("x").unwrap_or(0.0), num("y").unwrap_or(0.0));
                let (w, h) = (num("width").unwrap_or(0.0), num("height").unwrap_or(0.0));
                let r = num("rx").or(num("ry")).unwrap_or(0.0).min(w / 2.0).min(h / 2.0);
                parts.push(Part::Line { points: rounded_rect(x, y, w, h, r), closed: true });
            }
            "circle" => {
                let c = pos2(num("cx").unwrap_or(0.0), num("cy").unwrap_or(0.0));
                let r = num("r").unwrap_or(0.0);
                if attr(element, "fill").is_some_and(|f| f != "none") {
                    parts.push(Part::Dot { centre: c, radius: r });
                } else {
                    let points = (0..32).map(|i| c + angle(i as f32 / 32.0 * TAU) * r).collect();
                    parts.push(Part::Line { points, closed: true });
                }
            }
            _ => {}
        }
    }
    parts
}

fn attr<'a>(element: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = element;
    while let Some(i) = rest.find(key) {
        let before_ok = i == 0 || rest.as_bytes()[i - 1].is_ascii_whitespace();
        let after = &rest[i + key.len()..];
        if before_ok {
            if let Some(value) = after.trim_start().strip_prefix('=') {
                let value = value.trim_start().strip_prefix('"')?;
                return value.split('"').next();
            }
        }
        rest = after;
    }
    None
}

fn angle(a: f32) -> Vec2 {
    vec2(a.cos(), a.sin())
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, r: f32) -> Vec<Pos2> {
    if r <= 0.0 {
        return vec![pos2(x, y), pos2(x + w, y), pos2(x + w, y + h), pos2(x, y + h)];
    }
    let corners = [
        (pos2(x + w - r, y + r), -0.5 * PI),
        (pos2(x + w - r, y + h - r), 0.0),
        (pos2(x + r, y + h - r), 0.5 * PI),
        (pos2(x + r, y + r), PI),
    ];
    let mut points = Vec::new();
    for (c, start) in corners {
        for i in 0..=6 {
            points.push(c + angle(start + i as f32 / 6.0 * 0.5 * PI) * r);
        }
    }
    points
}

/// Number and flag tokens of a path's data.
struct Tokens<'a> {
    s: &'a [u8],
    i: usize,
}

impl Tokens<'_> {
    fn skip_separators(&mut self) {
        while self.i < self.s.len() && (self.s[self.i].is_ascii_whitespace() || self.s[self.i] == b',') {
            self.i += 1;
        }
    }

    fn command(&mut self) -> Option<u8> {
        self.skip_separators();
        let c = *self.s.get(self.i)?;
        if c.is_ascii_alphabetic() {
            self.i += 1;
            Some(c)
        } else {
            None
        }
    }

    fn has_number(&mut self) -> bool {
        self.skip_separators();
        self.s.get(self.i).is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> f32 {
        self.skip_separators();
        let start = self.i;
        let mut seen_dot = false;
        let mut seen_exp = false;
        if matches!(self.s.get(self.i), Some(b'-' | b'+')) {
            self.i += 1;
        }
        while let Some(&c) = self.s.get(self.i) {
            match c {
                b'0'..=b'9' => {}
                b'.' if !seen_dot && !seen_exp => seen_dot = true,
                b'e' | b'E' if !seen_exp => {
                    seen_exp = true;
                    if matches!(self.s.get(self.i + 1), Some(b'-' | b'+')) {
                        self.i += 1;
                    }
                }
                _ => break,
            }
            self.i += 1;
        }
        std::str::from_utf8(&self.s[start..self.i]).ok().and_then(|t| t.parse().ok()).unwrap_or(0.0)
    }

    /// Arc flags are single digits and may be packed without separators ("011").
    fn flag(&mut self) -> bool {
        self.skip_separators();
        let c = self.s.get(self.i).copied();
        self.i += 1;
        c == Some(b'1')
    }

    fn point(&mut self) -> Vec2 {
        let x = self.number();
        let y = self.number();
        vec2(x, y)
    }
}

/// SVG path data -> polylines. Supports M L H V C S Q T A Z, absolute and relative.
pub fn path(d: &str) -> Vec<Part> {
    let mut t = Tokens { s: d.as_bytes(), i: 0 };
    let mut parts = Vec::new();
    let mut current: Vec<Pos2> = Vec::new();
    let mut pen = Pos2::ZERO;
    let mut start = Pos2::ZERO;
    let mut last_control: Option<Pos2> = None;
    let mut command = b'M';

    let finish = |current: &mut Vec<Pos2>, parts: &mut Vec<Part>, closed: bool| {
        if current.len() > 1 {
            parts.push(Part::Line { points: std::mem::take(current), closed });
        }
        current.clear();
    };

    loop {
        if let Some(c) = t.command() {
            command = c;
        } else if !t.has_number() {
            break;
        }
        let relative = command.is_ascii_lowercase();
        let base = if relative { pen.to_vec2() } else { Vec2::ZERO };
        let mut control = None;
        match command.to_ascii_uppercase() {
            b'M' => {
                finish(&mut current, &mut parts, false);
                pen = (base + t.point()).to_pos2();
                start = pen;
                current.push(pen);
                // Further pairs after a move are implicit line-tos.
                command = if relative { b'l' } else { b'L' };
            }
            b'L' => {
                pen = (base + t.point()).to_pos2();
                current.push(pen);
            }
            b'H' => {
                let x = t.number();
                pen.x = if relative { pen.x + x } else { x };
                current.push(pen);
            }
            b'V' => {
                let y = t.number();
                pen.y = if relative { pen.y + y } else { y };
                current.push(pen);
            }
            b'C' | b'S' => {
                let c1 = if command.eq_ignore_ascii_case(&b'C') {
                    (base + t.point()).to_pos2()
                } else {
                    last_control.map_or(pen, |c| pen + (pen - c))
                };
                let c2 = (base + t.point()).to_pos2();
                let end = (base + t.point()).to_pos2();
                for i in 1..=12 {
                    current.push(cubic(pen, c1, c2, end, i as f32 / 12.0));
                }
                control = Some(c2);
                pen = end;
            }
            b'Q' | b'T' => {
                let c = if command.eq_ignore_ascii_case(&b'Q') {
                    (base + t.point()).to_pos2()
                } else {
                    last_control.map_or(pen, |c| pen + (pen - c))
                };
                let end = (base + t.point()).to_pos2();
                for i in 1..=10 {
                    let s = i as f32 / 10.0;
                    current.push(pen.lerp(c, s).lerp(c.lerp(end, s), s));
                }
                control = Some(c);
                pen = end;
            }
            b'A' => {
                let (rx, ry) = (t.number(), t.number());
                let rotation = t.number().to_radians();
                let (large, sweep) = (t.flag(), t.flag());
                let end = (base + t.point()).to_pos2();
                current.extend(arc(pen, rx, ry, rotation, large, sweep, end));
                pen = end;
            }
            b'Z' => {
                pen = start;
                finish(&mut current, &mut parts, true);
                current.push(pen);
            }
            _ => break,
        }
        last_control = control;
    }
    finish(&mut current, &mut parts, false);

    // A zero-length stroke with a round cap is a dot (Lucide's "i" in info).
    parts
        .into_iter()
        .map(|p| match p {
            Part::Line { points, closed: false } if length(&points) < 0.1 => {
                Part::Dot { centre: points[0], radius: 1.0 }
            }
            other => other,
        })
        .collect()
}

fn length(points: &[Pos2]) -> f32 {
    points.windows(2).map(|w| w[0].distance(w[1])).sum()
}

fn cubic(p0: Pos2, p1: Pos2, p2: Pos2, p3: Pos2, s: f32) -> Pos2 {
    let u = 1.0 - s;
    let v = p0.to_vec2() * (u * u * u) + p1.to_vec2() * (3.0 * u * u * s) + p2.to_vec2() * (3.0 * u * s * s) + p3.to_vec2() * (s * s * s);
    v.to_pos2()
}

/// Endpoint arc -> points after `from` (SVG spec, appendix B.2.4).
fn arc(from: Pos2, rx: f32, ry: f32, phi: f32, large: bool, sweep: bool, to: Pos2) -> Vec<Pos2> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx < 1e-6 || ry < 1e-6 || from == to {
        return vec![to];
    }
    let (sin, cos) = phi.sin_cos();
    let d = (from - to) / 2.0;
    let x1 = cos * d.x + sin * d.y;
    let y1 = -sin * d.x + cos * d.y;
    let lambda = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = (rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1).max(0.0);
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut k = (num / den).sqrt();
    if large == sweep {
        k = -k;
    }
    let cx1 = k * rx * y1 / ry;
    let cy1 = -k * ry * x1 / rx;
    let mid = (from.to_vec2() + to.to_vec2()) / 2.0;
    let centre = pos2(cos * cx1 - sin * cy1 + mid.x, sin * cx1 + cos * cy1 + mid.y);

    let vector_angle = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let dot = ux * vx + uy * vy;
        let len = (ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt();
        let a = (dot / len).clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 { -a } else { a }
    };
    let start = vector_angle(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut delta = vector_angle((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && delta > 0.0 {
        delta -= TAU;
    } else if sweep && delta < 0.0 {
        delta += TAU;
    }

    let steps = ((delta.abs() / TAU) * 40.0).ceil().max(2.0) as usize;
    (1..=steps)
        .map(|i| {
            let a = start + delta * i as f32 / steps as f32;
            let (s, c) = a.sin_cos();
            pos2(centre.x + rx * c * cos - ry * s * sin, centre.y + rx * c * sin + ry * s * cos)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(parts: &[Part]) -> Vec<&Vec<Pos2>> {
        parts.iter().filter_map(|p| if let Part::Line { points, .. } = p { Some(points) } else { None }).collect()
    }

    #[test]
    fn relative_line_commands() {
        let parts = path("m15 18-6-6 6-6");
        assert_eq!(lines(&parts)[0], &vec![pos2(15.0, 18.0), pos2(9.0, 12.0), pos2(15.0, 6.0)]);
    }

    #[test]
    fn horizontal_vertical_and_packed_numbers() {
        let parts = path("M21 3v5h-5");
        assert_eq!(lines(&parts)[0], &vec![pos2(21.0, 3.0), pos2(21.0, 8.0), pos2(16.0, 8.0)]);
        let mut t = Tokens { s: b"1.1.9-2", i: 0 };
        assert_eq!((t.number(), t.number(), t.number()), (1.1, 0.9, -2.0));
    }

    #[test]
    fn rotate_cw_arc_goes_round_the_bottom() {
        // Lucide rotate-cw: from (21,12) the long way round to (12,3), clockwise on screen.
        let parts = path("M21 12a9 9 0 1 1-9-9");
        let pts = lines(&parts)[0];
        let last = *pts.last().unwrap();
        assert!(last.distance(pos2(12.0, 3.0)) < 1e-3);
        // Every point is on the circle around (12,12)...
        assert!(pts.iter().all(|p| (p.distance(pos2(12.0, 12.0)) - 9.0).abs() < 1e-2));
        // ...and it passes through the bottom of it.
        assert!(pts.iter().any(|p| p.y > 20.9));
    }

    #[test]
    fn info_dot_becomes_a_dot() {
        let parts = parse(r#"<circle cx="12" cy="12" r="10" /><path d="M12 8h.01" />"#);
        assert!(matches!(parts[1], Part::Dot { .. }));
        assert!(matches!(parts[0], Part::Line { closed: true, .. }));
    }

    #[test]
    fn rect_attributes() {
        let parts = parse(r#"<rect width="7" height="7" x="3" y="3" rx="1" />"#);
        let pts = lines(&parts)[0];
        let min_x = pts.iter().map(|p| p.x).fold(f32::MAX, f32::min);
        let max_y = pts.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        assert!((min_x - 3.0).abs() < 1e-4 && (max_y - 10.0).abs() < 1e-4);
    }

    #[test]
    fn every_bundled_icon_parses() {
        for (name, svg) in crate::icons::SOURCES {
            assert!(!parse(svg).is_empty(), "{name} produced nothing");
        }
    }
}
