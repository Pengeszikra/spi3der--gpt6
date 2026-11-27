// Plane splits and BSP operations follow the pinned engine's csg.js.
use crate::{
    geometry::{compact, edge},
    scene::*,
};
use std::collections::BTreeMap;
#[derive(Clone)]
struct Poly {
    p: Vec<V>,
    n: V,
    w: f64,
    face: Face,
}
fn polygons(o: &Object) -> Vec<Poly> {
    let mut out = vec![];
    for f in &o.f {
        if f.v.len() < 3 || len(f.normal) < 0.5 {
            continue;
        }
        let poly = |ids: &[usize]| Poly {
            p: ids.iter().map(|&i| o.p[i]).collect(),
            n: f.normal,
            w: dot(f.normal, f.mid),
            face: Face {
                source: None,
                v: vec![],
                triangles: vec![],
                ..f.clone()
            },
        };
        if f.triangles.is_empty() {
            out.push(poly(&f.v))
        } else {
            out.extend(f.triangles.iter().map(|ids| poly(ids)))
        }
    }
    out
}
#[derive(Default)]
struct Split {
    front: Vec<Poly>,
    back: Vec<Poly>,
    same: Vec<Poly>,
    opposite: Vec<Poly>,
}
fn split(plane: (V, f64), p: Poly, s: &mut Split, eps: f64) {
    let (n, w) = plane;
    let d: Vec<_> = p.p.iter().map(|&v| dot(n, v) - w).collect();
    let front = d.iter().any(|&d| d > eps);
    let back = d.iter().any(|&d| d < -eps);
    match (front, back) {
        (false, false) => {
            if dot(n, p.n) > 0. {
                s.same.push(p)
            } else {
                s.opposite.push(p)
            }
        }
        (true, false) => s.front.push(p),
        (false, true) => s.back.push(p),
        (true, true) => {
            let (mut f, mut b) = (vec![], vec![]);
            for (i, &a) in p.p.iter().enumerate() {
                let j = (i + 1) % p.p.len();
                if d[i] >= -eps {
                    f.push(a)
                }
                if d[i] <= eps {
                    b.push(a)
                }
                if (d[i] > eps && d[j] < -eps) || (d[i] < -eps && d[j] > eps) {
                    let v = add(a, mul(sub(p.p[j], a), d[i] / (d[i] - d[j])));
                    f.push(v);
                    b.push(v)
                }
            }
            if f.len() >= 3 {
                s.front.push(Poly { p: f, ..p.clone() })
            }
            if b.len() >= 3 {
                s.back.push(Poly { p: b, ..p })
            }
        }
    }
}
#[derive(Default)]
struct Node {
    plane: Option<(V, f64)>,
    polys: Vec<Poly>,
    front: Option<Box<Node>>,
    back: Option<Box<Node>>,
}
impl Node {
    fn build(&mut self, polys: Vec<Poly>, eps: f64, depth: usize) -> Result<(), String> {
        let Some(first) = polys.first() else {
            return Ok(());
        };
        if depth > 384 {
            return Err("BSP exceeds supported nesting".into());
        }
        let plane = *self.plane.get_or_insert((first.n, first.w));
        let mut s = Split::default();
        for p in polys {
            split(plane, p, &mut s, eps)
        }
        self.polys.extend(s.same);
        self.polys.extend(s.opposite);
        for (child, polys) in [(&mut self.front, s.front), (&mut self.back, s.back)] {
            if !polys.is_empty() {
                child
                    .get_or_insert_with(Default::default)
                    .build(polys, eps, depth + 1)?
            }
        }
        Ok(())
    }
    fn invert(&mut self) {
        if let Some((n, w)) = &mut self.plane {
            *n = mul(*n, -1.);
            *w = -*w
        }
        for p in &mut self.polys {
            p.p.reverse();
            p.n = mul(p.n, -1.);
            p.w = -p.w
        }
        for n in [&mut self.front, &mut self.back].into_iter().flatten() {
            n.invert()
        }
        std::mem::swap(&mut self.front, &mut self.back)
    }
    fn clip(&self, polys: Vec<Poly>, eps: f64) -> Vec<Poly> {
        let Some(plane) = self.plane else {
            return polys;
        };
        if polys.is_empty() {
            return polys;
        }
        let mut s = Split::default();
        for p in polys {
            split(plane, p, &mut s, eps)
        }
        s.front.extend(s.same);
        s.back.extend(s.opposite);
        let mut out = if let Some(c) = &self.front {
            c.clip(s.front, eps)
        } else {
            s.front
        };
        if let Some(c) = &self.back {
            out.extend(c.clip(s.back, eps))
        }
        out
    }
    fn clip_to(&mut self, other: &Node, eps: f64) {
        self.polys = other.clip(std::mem::take(&mut self.polys), eps);
        for n in [&mut self.front, &mut self.back].into_iter().flatten() {
            n.clip_to(other, eps)
        }
    }
    fn drain(&mut self, out: &mut Vec<Poly>) {
        out.append(&mut self.polys);
        for n in [&mut self.front, &mut self.back].into_iter().flatten() {
            n.drain(out)
        }
    }
}
fn mesh(polys: Vec<Poly>, cell: f64) -> Object {
    let mut o = Object::default();
    let mut cells: BTreeMap<[i64; 3], usize> = BTreeMap::new();
    for p in polys {
        let mut f = p.face;
        for q in p.p {
            let k = q.map(|v| (v / cell).round() as i64);
            let mut hit = None;
            'neighbors: for x in -1..=1 {
                for y in -1..=1 {
                    for z in -1..=1 {
                        if let Some(&i) = cells.get(&[k[0] + x, k[1] + y, k[2] + z]) {
                            if len(sub(o.p[i], q)) <= 1.5 * cell {
                                hit = Some(i);
                                break 'neighbors;
                            }
                        }
                    }
                }
            }
            let v = hit.unwrap_or_else(|| {
                let i = o.p.len();
                o.p.push(q);
                cells.insert(k, i);
                i
            });
            if f.v.last() != Some(&v) {
                f.v.push(v)
            }
        }
        if f.v.len() > 1 && f.v.first() == f.v.last() {
            f.v.pop();
        }
        if f.v.len() >= 3 {
            o.f.push(f)
        }
    }
    for f in &mut o.f {
        let mut v = vec![];
        for (i, &a) in f.v.iter().enumerate() {
            let b = f.v[(i + 1) % f.v.len()];
            let d = sub(o.p[b], o.p[a]);
            let l = len(d);
            v.push(a);
            if l < 12. * cell {
                continue;
            }
            let mut on = vec![];
            for (j, &p) in o.p.iter().enumerate() {
                if f.v.contains(&j) {
                    continue;
                }
                let q = sub(p, o.p[a]);
                let t = dot(q, d) / (l * l);
                if t > 0. && t < 1. && len(cross(q, d)) / l <= 3. * cell {
                    on.push((t, j))
                }
            }
            on.sort_by(|p, q| p.0.total_cmp(&q.0));
            v.extend(on.into_iter().map(|p| p.1));
        }
        f.v = v
    }
    o.refresh();
    o.f.retain(|f| len(f.normal) > 0.5);
    compact(&mut o);
    o.refresh();
    o
}
pub fn closed(o: &Object) -> bool {
    let mut counts = BTreeMap::new();
    for f in &o.f {
        for (i, &a) in f.v.iter().enumerate() {
            let b = f.v[(i + 1) % f.v.len()];
            let entry = counts.entry(edge(a, b)).or_insert([0, 0]);
            entry[usize::from(a < b)] += 1;
        }
    }
    !counts.is_empty() && counts.values().all(|&v| v == [1, 1])
}
pub fn boolean(a: &Object, b: &Object, op: &str) -> Result<Object, String> {
    if !closed(a) || !closed(b) {
        return Err("boolean needs closed, consistently wound operands".into());
    }
    let scale = 2. * a.radius.max(b.radius).max(1e-9) + len(sub(a.center, b.center));
    let eps = scale * 1e-6;
    let (mut x, mut y) = (Node::default(), Node::default());
    x.build(polygons(a), eps, 0)?;
    y.build(polygons(b), eps, 0)?;
    match op {
        "union" | "subtract" => {
            if op == "subtract" {
                x.invert()
            }
            x.clip_to(&y, eps);
            y.clip_to(&x, eps);
            y.invert();
            y.clip_to(&x, eps);
            y.invert()
        }
        "intersect" => {
            x.invert();
            y.clip_to(&x, eps);
            y.invert();
            x.clip_to(&y, eps);
            y.clip_to(&x, eps)
        }
        _ => return Err("unknown boolean operation".into()),
    }
    let mut polys = vec![];
    y.drain(&mut polys);
    x.build(polys, eps, 0)?;
    if op != "union" {
        x.invert()
    }
    let mut polys = vec![];
    x.drain(&mut polys);
    let mut result = mesh(polys, scale * 1e-5);
    if !result.f.is_empty() && !closed(&result) {
        return Err("boolean would leave an open solid".into());
    }
    result.name = a.name.clone();
    Ok(result)
}
