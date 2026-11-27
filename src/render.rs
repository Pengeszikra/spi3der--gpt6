use crate::{camera::*, scene::*};
pub struct Frame {
    pub w: usize,
    pub h: usize,
    pub rgb: Vec<[u8; 3]>,
    pub ids: Vec<Option<(usize, usize)>>,
    pub z: Vec<f64>,
}
impl Frame {
    pub fn new(w: usize, h: usize) -> Self {
        Self {
            w,
            h,
            rgb: vec![[12, 14, 18]; w * h],
            ids: vec![None; w * h],
            z: vec![f64::NEG_INFINITY; w * h],
        }
    }
    fn tri(&mut self, a: V, b: V, c: V, col: [u8; 3], id: (usize, usize)) {
        let edge =
            |a: V, b: V, x: f64, y: f64| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
        let area = edge(a, b, c[0], c[1]);
        if area.abs() < 1e-10 {
            return;
        }
        let lo = |i: usize| a[i].min(b[i]).min(c[i]).floor().max(0.) as usize;
        let hi = |i: usize, limit: usize| {
            a[i].max(b[i]).max(c[i]).ceil().clamp(0., limit as f64) as usize
        };
        for y in lo(1)..hi(1, self.h) {
            for x in lo(0)..hi(0, self.w) {
                let u = edge(b, c, x as f64 + 0.5, y as f64 + 0.5) / area;
                let v = edge(c, a, x as f64 + 0.5, y as f64 + 0.5) / area;
                let t = 1. - u - v;
                if u >= -1e-9 && v >= -1e-9 && t >= -1e-9 {
                    let z = u * a[2] + v * b[2] + t * c[2];
                    let k = y * self.w + x;
                    if z > self.z[k] {
                        self.z[k] = z;
                        self.rgb[k] = col;
                        self.ids[k] = Some(id)
                    }
                }
            }
        }
    }
    fn pair(&self, x: usize, y: usize) -> [[u8; 3]; 2] {
        [
            self.rgb[y * self.w + x],
            self.rgb[(y + 1).min(self.h - 1) * self.w + x],
        ]
    }
    pub fn span(&self, y: usize, start: usize, end: usize, out: &mut String) {
        use std::fmt::Write;
        let mut fg = None;
        let mut bg = None;
        for x in start..end {
            let [a, b] = self.pair(x, y);
            let solid = a == b;
            if !solid && fg != Some(a) {
                write!(out, "\x1b[38;2;{};{};{}m", a[0], a[1], a[2]).unwrap();
                fg = Some(a)
            }
            if bg != Some(b) {
                write!(out, "\x1b[48;2;{};{};{}m", b[0], b[1], b[2]).unwrap();
                bg = Some(b)
            }
            out.push(if solid { ' ' } else { '▀' })
        }
        out.push_str("\x1b[0m");
    }
    pub fn rows(&self) -> Vec<String> {
        (0..self.h)
            .step_by(2)
            .map(|y| {
                let mut row = String::new();
                self.span(y, 0, self.w, &mut row);
                row
            })
            .collect()
    }
    pub fn diff(&self, prev: Option<&Frame>, out: &mut String) {
        use std::fmt::Write;
        let prev = prev.filter(|p| p.w == self.w && p.h == self.h);
        for y in (0..self.h).step_by(2) {
            let same = |x| prev.is_some_and(|p| p.pair(x, y) == self.pair(x, y));
            let mut x = 0;
            while x < self.w {
                if same(x) {
                    x += 1;
                    continue;
                }
                let start = x;
                let mut end = x + 1;
                x = end;
                while x < self.w && x - end <= 3 {
                    if !same(x) {
                        end = x + 1
                    }
                    x += 1
                }
                write!(out, "\x1b[{};{}H", y / 2 + 1, start + 1).unwrap();
                self.span(y, start, end, out);
            }
        }
    }
}
pub fn render(
    s: &Scene,
    v: &View,
    w: usize,
    h: usize,
    selected: Option<usize>,
    face: Option<usize>,
) -> Frame {
    let mut f = Frame::new(w, h);
    let cam = v.camera(w, h);
    if s.sky {
        let mut best = vec![0.; w * h];
        let mut star = |at: V, rgb: [u8; 3], b: f64, r: i32, id: Option<(usize, usize)>| {
            let q = cam.project(at);
            if q[2] <= 0.
                || q[0] < -2.
                || q[1] < -2.
                || q[0] > w as f64 + 2.
                || q[1] > h as f64 + 2.
            {
                return;
            }
            let x = q[0].floor() as i32;
            let y = q[1].floor() as i32;
            for dy in -r..=r {
                for dx in -r..=r {
                    let (x, y) = (x + dx, y + dy);
                    if x < 0 || y < 0 || x >= w as i32 || y >= h as i32 {
                        continue;
                    }
                    let k = y as usize * w + x as usize;
                    let b = if dx == 0 && dy == 0 { b } else { b * 0.55 };
                    if b > best[k] {
                        best[k] = b;
                        f.rgb[k] = rgb.map(|c| (c as f64 * b).round() as u8);
                        f.ids[k] = id;
                        f.z[k] = 1. / q[2];
                    }
                }
            }
        };
        for &p in s.dust.iter() {
            star(p, [150, 160, 200], 0.22, 0, None)
        }
        for (i, o) in s.objects.iter().enumerate() {
            if let Some(p) = &o.star {
                star(
                    o.center,
                    p.rgb,
                    p.brightness,
                    i32::from(p.big),
                    Some((i, 0)),
                )
            }
        }
    }
    let light = norm(add(
        mul(cam.f, -0.55),
        add(mul(cam.r, -0.35), mul(cam.u, 0.75)),
    ));
    for (oi, o) in s.objects.iter().enumerate() {
        if o.p.is_empty() {
            continue;
        }
        let p: Vec<_> = o.p.iter().map(|&p| cam.project(p)).collect();
        for e in &o.wires {
            f.line(p[e[0]], p[e[1]], [200, 200, 200]);
        }
        for (fi, poly) in o.f.iter().enumerate() {
            if poly.v.len() < 3 {
                continue;
            }
            let n = if dot(poly.normal, sub(cam.eye, poly.mid)) < 0. {
                mul(poly.normal, -1.)
            } else {
                poly.normal
            };
            let shade = (0.32 + 0.85 * dot(n, light).max(0.) + poly.emit * 0.4).min(1.25);
            let col = poly.color.map(|x| {
                let c = x as f64 * shade;
                (if selected == Some(oi) && face == Some(fi) {
                    c * 0.75 + 70.
                } else {
                    c
                })
                .round() as u8
            });
            for t in 1..poly.v.len() - 1 {
                let ids = poly.triangles.get(t - 1).copied().unwrap_or([
                    poly.v[0],
                    poly.v[t],
                    poly.v[t + 1],
                ]);
                let points = ids.map(|i| p[i]);
                if points.iter().all(|q| q[2] > 1e-12) {
                    let [a, b, c] =
                        points.map(|q| [q[0], q[1], if cam.ortho { -q[2] } else { 1. / q[2] }]);
                    f.tri(a, b, c, col, (oi, fi))
                } else if points.iter().any(|q| q[2] > 1e-12) {
                    let mut clipped = Vec::with_capacity(4);
                    for j in 0..3 {
                        let k = (j + 1) % 3;
                        let (a, b) = (points[j][2], points[k][2]);
                        if a > 1e-12 {
                            clipped.push(points[j])
                        }
                        if (a > 1e-12) != (b > 1e-12) {
                            let v = add(
                                o.p[ids[j]],
                                mul(sub(o.p[ids[k]], o.p[ids[j]]), (1e-12 - a) / (b - a)),
                            );
                            clipped.push(cam.project(v))
                        }
                    }
                    for i in 1..clipped.len() - 1 {
                        let [a, b, c] = [clipped[0], clipped[i], clipped[i + 1]].map(|q| {
                            [
                                q[0],
                                q[1],
                                if cam.ortho {
                                    -q[2]
                                } else {
                                    1. / q[2].max(1e-15)
                                },
                            ]
                        });
                        f.tri(a, b, c, col, (oi, fi))
                    }
                }
            }
        }
    }
    if v.edges && !s.sky {
        let depth = |z: f64| if cam.ortho { -z } else { 1. / z };
        let jump = 0.04 * v.radius / v.zoom;
        for k in 0..f.ids.len() {
            let Some((o, i)) = f.ids[k] else { continue };
            let x = k % w;
            let y = k / w;
            let neighbors = [
                (x > 0).then(|| k - 1),
                (x + 1 < w).then_some(k + 1),
                (y > 0).then(|| k - w),
                (y + 1 < h).then_some(k + w),
            ];
            if neighbors.into_iter().flatten().any(|j| match f.ids[j] {
                None => true,
                Some(id) if id == (o, i) => false,
                Some((oo, ii)) => {
                    let d = depth(f.z[j]) - depth(f.z[k]);
                    d >= -jump
                        && (d > jump
                            || dot(s.objects[o].f[i].normal, s.objects[oo].f[ii].normal) < 0.6)
                }
            }) {
                f.rgb[k] = f.rgb[k].map(|c| c.div_ceil(2))
            }
        }
    }
    f
}
impl Frame {
    pub fn hit(&self, x: usize, y: usize) -> Option<(usize, usize)> {
        if x >= self.w || y >= self.h {
            return None;
        }
        self.ids[y * self.w + x].or_else(|| self.ids[((y + 1).min(self.h - 1)) * self.w + x])
    }
    pub fn mark(&mut self, q: V, c: [u8; 3]) {
        if q[2] > 0. && q[0] >= 0. && q[1] >= 0. && q[0] < self.w as f64 && q[1] < self.h as f64 {
            self.rgb[q[1] as usize * self.w + q[0] as usize] = c;
        }
    }
    pub fn line(&mut self, a: V, b: V, col: [u8; 3]) {
        if a[2] <= 0. || b[2] <= 0. {
            return;
        }
        let n = (b[0] - a[0])
            .abs()
            .max((b[1] - a[1]).abs())
            .ceil()
            .clamp(1., (self.w + self.h) as f64 * 2.) as usize;
        for i in 0..=n {
            let q = add(a, mul(sub(b, a), i as f64 / n as f64));
            self.mark(q, col)
        }
    }
}
