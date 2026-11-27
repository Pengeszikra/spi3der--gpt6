use crate::{camera::Camera, geometry::*, render::Frame, scene::*, App};
pub fn segment_distance(p: [f64; 2], a: V, b: V) -> f64 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let t =
        (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy).max(1e-12)).clamp(0., 1.);
    (p[0] - a[0] - dx * t).hypot(p[1] - a[1] - dy * t)
}
pub fn pick(a: &App, frame: &Frame, x: usize, y: usize) -> Option<Part> {
    let i = a.active().ok()?;
    let o = &a.scene.objects[i];
    let cam = a.view.camera(frame.w, frame.h);
    let m = [x as f64 + 0.5, y as f64 + 0.5];
    if a.sel.mode == 3 {
        return frame
            .hit(x, y)
            .and_then(|(oi, fi)| (oi == i && fi < o.f.len()).then_some(Part::Polygon(fi)));
    }
    let mut best = None;
    let mut closest = 3.5;
    for part in a.sel.candidates(o) {
        let d = match part {
            Part::Point(i) => {
                let q = cam.project(o.p[i]);
                if q[2] <= 0. {
                    continue;
                }
                (m[0] - q[0]).hypot(m[1] - q[1])
            }
            Part::Line([i, j]) => {
                let (q, r) = (cam.project(o.p[i]), cam.project(o.p[j]));
                if q[2] <= 0. || r[2] <= 0. {
                    continue;
                }
                segment_distance(m, q, r)
            }
            _ => continue,
        };
        if d < closest {
            best = Some(part);
            closest = d
        }
    }
    best
}
pub fn inside(poly: &[[f64; 2]], p: V) -> bool {
    if poly.len() < 3 || p[2] <= 0. {
        return false;
    }
    let mut yes = false;
    for (i, a) in poly.iter().enumerate() {
        let b = poly[(i + 1) % poly.len()];
        if (a[1] > p[1]) != (b[1] > p[1])
            && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
        {
            yes = !yes
        }
    }
    yes
}
pub fn region(a: &mut App, path: &[[f64; 2]], append: bool) {
    let Ok(i) = a.active() else { return };
    let o = &a.scene.objects[i];
    let cam = a.view.camera(a.size.0, a.size.1);
    if !append {
        a.sel.parts.clear()
    }
    for part in a.sel.candidates(o) {
        let hit = match part {
            Part::Point(i) => inside(path, cam.project(o.p[i])),
            Part::Line(e) => e.iter().all(|&i| inside(path, cam.project(o.p[i]))),
            Part::Polygon(i) => inside(path, cam.project(o.f[i].mid)),
        };
        if hit {
            a.sel.parts.insert(part);
        }
    }
}
pub fn world(a: &App, f: &Frame, x: usize, y: usize) -> V {
    let cam = a.view.camera(f.w, f.h);
    let k = y.min(f.h - 1) * f.w + x.min(f.w - 1);
    let z = if f.z[k].is_finite() {
        if cam.ortho {
            -f.z[k]
        } else {
            1. / f.z[k]
        }
    } else {
        a.target
            .and_then(|i| a.scene.objects.get(i))
            .map_or(1., |o| cam.project(o.center)[2])
            .max(0.001)
    };
    cam.point(x as f64 + 0.5, y as f64 + 0.5, z)
}
pub struct Transform {
    pub op: char,
    pub axis: Option<usize>,
    pub number: String,
    pub segments: usize,
    pub start: [usize; 2],
    pub pos: [usize; 2],
    pub original: Object,
    pub selection: Selection,
    pub back: bool,
    pub minus: bool,
}
impl Transform {
    pub fn new(a: &App, op: char, mouse: [usize; 2], back: bool) -> Result<Self, String> {
        let original = a.scene.objects[a.active()?].clone();
        let mut selection = a.sel.clone();
        if selection.parts.is_empty() && matches!(op, 'g' | 'r' | 's' | 'm') {
            selection.parts = selection.candidates(&original).into_iter().collect()
        }
        if selection.parts.is_empty() {
            return Err("select geometry first".into());
        }
        Ok(Self {
            op,
            axis: None,
            number: String::new(),
            segments: 1,
            start: mouse,
            pos: mouse,
            original,
            selection,
            back,
            minus: false,
        })
    }
    pub fn apply(&self, cam: &Camera, surface: &Face) -> Result<(Object, Selection), String> {
        let mut o = self.original.clone();
        let mut sel = self.selection.clone();
        let ids = sel.vertices(&o);
        let center = mul(
            ids.iter().fold([0.; 3], |p, &i| add(p, o.p[i])),
            1. / ids.len().max(1) as f64,
        );
        let depth = cam.project(center)[2].max(1e-10);
        let pt = |p: [usize; 2]| cam.point(p[0] as f64, p[1] as f64, depth);
        let delta = sub(pt(self.pos), pt(self.start));
        let dx = self.pos[0] as f64 - self.start[0] as f64;
        let dy = self.pos[1] as f64 - self.start[1] as f64;
        let typed = if self.number.is_empty() {
            None
        } else {
            Some(
                self.number
                    .parse::<f64>()
                    .ok()
                    .filter(|n| n.is_finite())
                    .ok_or("enter a finite number")?,
            )
        };
        let axis = self.axis.unwrap_or(1);
        match self.op {
            'g' => {
                let d = if self.axis.is_some() {
                    let mut v = [0.; 3];
                    v[axis] = typed.unwrap_or(delta[axis]);
                    v
                } else if let Some(n) = typed {
                    mul(cam.r, n)
                } else {
                    delta
                };
                for i in ids {
                    o.p[i] = add(o.p[i], d)
                }
            }
            's' => o.transform(
                "s",
                self.axis.unwrap_or(3),
                typed.unwrap_or((dx * 0.025 - dy * 0.0125).exp()),
                &ids,
            ),
            'r' => {
                let angle = typed.unwrap_or(dx * 3. - dy);
                let mut axis_dir = mul(cam.f, -1.);
                let mut pivot = center;
                let mut move_ids = ids;
                if let Some(i) = self.axis {
                    axis_dir = [0.; 3];
                    axis_dir[i] = 1.
                } else if let Some(Part::Line([i, j])) =
                    sel.parts.iter().find(|p| matches!(p, Part::Line(_)))
                {
                    pivot = o.p[*i];
                    axis_dir = norm(sub(o.p[*j], pivot));
                    if sel.parts.len() == 1 {
                        move_ids = (0..o.p.len()).collect()
                    }
                }
                let (s, c) = angle.to_radians().sin_cos();
                for i in move_ids {
                    let p = sub(o.p[i], pivot);
                    o.p[i] = add(
                        pivot,
                        add(
                            mul(p, c),
                            add(
                                mul(cross(axis_dir, p), s),
                                mul(axis_dir, dot(axis_dir, p) * (1. - c)),
                            ),
                        ),
                    )
                }
            }
            'e' => {
                sel = extrude(
                    &mut o,
                    &sel,
                    typed.unwrap_or(if dy == 0. { dx } else { -dy })
                        * if typed.is_some() { 1. } else { depth / cam.k },
                    self.axis,
                    self.back,
                    surface,
                )?
            }
            'm' => {
                mirror(
                    &mut o,
                    &sel,
                    self.axis.ok_or("choose X, Y or Z")?,
                    if self.minus { -1. } else { 1. },
                    typed.unwrap_or(delta[axis]),
                )?;
                sel.parts.clear()
            }
            'b' => {
                bevel(&mut o, &sel, typed.unwrap_or(len(delta)), self.segments)?;
                sel.parts.clear()
            }
            'l' => {
                let e = sel
                    .parts
                    .iter()
                    .find_map(|p| {
                        if let Part::Line(e) = p {
                            Some(*e)
                        } else {
                            None
                        }
                    })
                    .ok_or("select an edge")?;
                let count = typed.unwrap_or(1.);
                if count.fract() != 0. || !(1. ..=128.).contains(&count) {
                    return Err("expected 1–128 cuts".into());
                }
                sel = loop_cut(&mut o, e, count as usize, dx / 100.)?
            }
            _ => return Err("unknown edit tool".into()),
        }
        o.refresh();
        Ok((o, sel))
    }
}
