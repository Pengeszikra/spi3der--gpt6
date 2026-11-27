use crate::scene::*;
use std::collections::{BTreeMap, BTreeSet};
pub type Edge = [usize; 2];
pub fn edge(a: usize, b: usize) -> Edge {
    [a.min(b), a.max(b)]
}
pub fn edges(face: &Face) -> impl Iterator<Item = Edge> + '_ {
    face.v
        .iter()
        .enumerate()
        .map(|(k, &a)| edge(a, face.v[(k + 1) % face.v.len()]))
}
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Part {
    Point(usize),
    Line(Edge),
    Polygon(usize),
}
#[derive(Clone, Default)]
pub struct Selection {
    pub mode: u8,
    pub parts: BTreeSet<Part>,
}
impl Selection {
    pub fn vertices(&self, o: &Object) -> Vec<usize> {
        let mut out = BTreeSet::new();
        for part in &self.parts {
            match part {
                Part::Point(i) => {
                    out.insert(*i);
                }
                Part::Line(e) => out.extend(e),
                Part::Polygon(i) => out.extend(o.f[*i].v.iter()),
            }
        }
        out.into_iter().collect()
    }
    pub fn candidates(&self, o: &Object) -> Vec<Part> {
        match self.mode {
            1 => (0..o.p.len()).map(Part::Point).collect(),
            2 => {
                o.f.iter()
                    .flat_map(edges)
                    .chain(o.wires.iter().copied())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .map(Part::Line)
                    .collect()
            }
            _ => (0..o.f.len()).map(Part::Polygon).collect(),
        }
    }
    pub fn faces(&self) -> Vec<usize> {
        self.parts
            .iter()
            .filter_map(|p| {
                if let Part::Polygon(i) = p {
                    Some(*i)
                } else {
                    None
                }
            })
            .collect()
    }
}
fn push_face(o: &mut Object, v: Vec<usize>, surface: &Face) {
    o.f.push(Face {
        v,
        ..surface.clone()
    })
}
fn weld(o: &mut Object, p: V) -> usize {
    let eps = (o.radius * 1e-8).max(1e-10);
    if let Some(i) = o.p.iter().position(|&q| len(sub(p, q)) < eps) {
        i
    } else {
        o.p.push(p);
        o.p.len() - 1
    }
}
fn ordered(loop_edges: &BTreeSet<Edge>) -> Result<Vec<usize>, String> {
    let mut pending = loop_edges.clone();
    let [start, mut current] = pending.pop_first().ok_or("empty boundary")?;
    let mut ring = vec![start];
    while current != start {
        ring.push(current);
        let link = pending
            .iter()
            .find(|e| e.contains(&current))
            .copied()
            .ok_or("boundary is open")?;
        pending.remove(&link);
        current = if link[0] == current { link[1] } else { link[0] };
    }
    if !pending.is_empty() {
        return Err("multiple boundaries".into());
    }
    Ok(ring)
}
pub fn fill(o: &mut Object, sel: &Selection, surface: &Face) -> Result<(), String> {
    let ids = sel.vertices(o);
    if ids.len() < 3 {
        return Err("select at least three boundary vertices".into());
    }
    let mut uses = BTreeMap::new();
    for e in o.f.iter().flat_map(edges) {
        *uses.entry(e).or_insert(0) += 1
    }
    let boundary = uses
        .iter()
        .filter(|(e, n)| **n == 1 && e.iter().all(|v| ids.contains(v)))
        .map(|(e, _)| *e)
        .chain(
            o.wires
                .iter()
                .filter(|e| e.iter().all(|v| ids.contains(v)))
                .copied(),
        )
        .collect();
    let mut ring = ordered(&boundary).or_else(|_| {
        if ids.len() == 3 {
            Ok(ids)
        } else {
            Err("select one connected boundary".to_string())
        }
    })?;
    if let Some(f) =
        o.f.iter()
            .find(|f| edges(f).any(|e| e == edge(ring[0], ring[1])))
    {
        if f.v
            .iter()
            .enumerate()
            .any(|(i, &v)| v == ring[0] && f.v[(i + 1) % f.v.len()] == ring[1])
        {
            ring.reverse()
        }
    }
    let ring_edges: BTreeSet<_> = (0..ring.len())
        .map(|i| edge(ring[i], ring[(i + 1) % ring.len()]))
        .collect();
    let skin =
        o.f.iter()
            .find(|f| edges(f).any(|e| ring_edges.contains(&e)))
            .unwrap_or(surface)
            .clone();
    push_face(o, ring, &skin);
    o.refresh();
    Ok(())
}
pub fn extrude(
    o: &mut Object,
    sel: &Selection,
    d: f64,
    axis: Option<usize>,
    back: bool,
    surface: &Face,
) -> Result<Selection, String> {
    if sel.parts.is_empty() {
        return Err("select geometry first".into());
    }
    let faces = sel.faces();
    let mut outward = [0.; 3];
    for &i in &faces {
        outward = add(outward, o.f[i].normal)
    }
    let mut dir = norm(outward);
    if let Some(a) = axis {
        dir = [0.; 3];
        dir[a] = 1.
    } else if len(dir) < 0.01 {
        dir = [0., 1., 0.]
    }
    let mut lift = BTreeMap::new();
    for i in sel.vertices(o) {
        lift.insert(i, o.p.len());
        o.p.push(add(o.p[i], mul(dir, d)))
    }
    let mut counts = BTreeMap::new();
    for &i in &faces {
        for e in edges(&o.f[i]) {
            *counts.entry(e).or_insert(0) += 1
        }
    }
    let mut next = Selection {
        mode: sel.mode,
        ..Selection::default()
    };
    for &part in &sel.parts {
        match part {
            Part::Point(i) => {
                o.wires.push([i, lift[&i]]);
                next.parts.insert(Part::Point(lift[&i]));
            }
            Part::Line([a, b]) => {
                push_face(o, vec![a, b, lift[&b], lift[&a]], surface);
                next.parts.insert(Part::Line(edge(lift[&a], lift[&b])));
            }
            Part::Polygon(i) => {
                let base = o.f[i].clone();
                for (j, &a) in base.v.iter().enumerate() {
                    let b = base.v[(j + 1) % base.v.len()];
                    if counts[&edge(a, b)] == 1 {
                        push_face(o, vec![a, b, lift[&b], lift[&a]], &base)
                    }
                }
                o.f[i].v = base.v.iter().map(|v| lift[v]).collect();
                if back {
                    let mut cap = base;
                    cap.v.reverse();
                    o.f.push(cap)
                }
                next.parts.insert(Part::Polygon(i));
            }
        }
    }
    o.refresh();
    Ok(next)
}
pub fn lathe(
    name: &str,
    profile: &[[f64; 2]],
    origin: V,
    axis: V,
    up: V,
    n: usize,
    surface: &Face,
) -> Result<Object, String> {
    if !(3..=512).contains(&n)
        || profile.len() < 2
        || profile
            .iter()
            .any(|p| p[1] < 0. || !p.iter().all(|v| v.is_finite()))
    {
        return Err("invalid lathe contour or segments".into());
    }
    let mut o = Object {
        name: name.into(),
        ..Object::default()
    };
    let side = cross(axis, up);
    let mut rings = vec![];
    for &[u, r] in profile {
        let mut ring = vec![];
        for j in 0..if r < 1e-12 { 1 } else { n } {
            let (s, c) = (std::f64::consts::TAU * j as f64 / n as f64).sin_cos();
            ring.push(o.p.len());
            o.p.push(add(
                origin,
                add(mul(axis, u), mul(add(mul(up, c), mul(side, s)), r)),
            ))
        }
        rings.push(ring)
    }
    for pair in rings.windows(2) {
        let (a, b) = (&pair[0], &pair[1]);
        if a.len() + b.len() == 2 {
            continue;
        }
        for j in 0..n {
            let k = (j + 1) % n;
            let mut v = vec![
                a[j % a.len()],
                a[k % a.len()],
                b[k % b.len()],
                b[j % b.len()],
            ];
            v.dedup();
            if v.first() == v.last() {
                v.pop();
            }
            push_face(&mut o, v, surface)
        }
    }
    if rings[0].len() > 2 {
        let mut cap = rings[0].clone();
        cap.reverse();
        push_face(&mut o, cap, surface)
    }
    if rings.last().unwrap().len() > 2 {
        push_face(&mut o, rings.pop().unwrap(), surface)
    }
    o.refresh();
    let volume: f64 =
        o.f.iter()
            .map(|f| {
                let a = o.p[f.v[0]];
                (1..f.v.len() - 1)
                    .map(|i| dot(a, cross(o.p[f.v[i]], o.p[f.v[i + 1]])))
                    .sum::<f64>()
            })
            .sum();
    if volume < 0. {
        for f in &mut o.f {
            f.v.reverse()
        }
        o.refresh()
    }
    Ok(o)
}
pub fn primitive(kind: &str, name: &str, surface: &Face) -> Result<Object, String> {
    if matches!(kind, "box" | "cube") {
        let mut o = Object::cube(name.into(), surface.color);
        for f in &mut o.f {
            f.mat = surface.mat.clone();
            f.emit = surface.emit
        }
        return Ok(o);
    }
    let (n, profile) = match kind {
        "cylinder" => (24, vec![[0., 0.5], [1., 0.5]]),
        "cone" => (24, vec![[0., 0.5], [1., 0.]]),
        "pyramid" => (4, vec![[0., 0.7], [1., 0.]]),
        "prism" => (3, vec![[0., 0.6], [1., 0.6]]),
        "sphere" => (
            24,
            (0..=12)
                .map(|i| {
                    let a = std::f64::consts::PI * i as f64 / 12.;
                    [(1. - a.cos()) / 2., a.sin() / 2.]
                })
                .collect(),
        ),
        _ => return Err("unknown primitive".into()),
    };
    lathe(
        name,
        &profile,
        [0.; 3],
        [0., 1., 0.],
        [1., 0., 0.],
        n,
        surface,
    )
}
fn clipped(o: &mut Object, f: &Face, at: V, n: V) -> Vec<usize> {
    let mut result = vec![];
    let eps = o.radius.max(1.) * 1e-10;
    for (i, &a) in f.v.iter().enumerate() {
        let b = f.v[(i + 1) % f.v.len()];
        let (p, q) = (o.p[a], o.p[b]);
        let (d, e) = (dot(sub(p, at), n), dot(sub(q, at), n));
        if d >= -eps {
            result.push(a)
        }
        if (d > eps && e < -eps) || (d < -eps && e > eps) {
            result.push(weld(o, add(p, mul(sub(q, p), d / (d - e)))))
        }
    }
    result.dedup();
    if result.first() == result.last() {
        result.pop();
    }
    result
}
pub fn saw(o: &Object, at: V, n: V, surface: &Face) -> Result<[Object; 2], String> {
    if !crate::csg::closed(o) || len(n) < 1e-12 {
        return Err("select a closed solid and a cutting plane".into());
    }
    if let Ok(halves) = saw_contour(o, at, n, surface) {
        if halves.iter().all(crate::csg::closed) {
            return Ok(halves);
        }
    }
    let n = norm(n);
    let u = norm(cross(
        if n[1].abs() < 0.9 {
            [0., 1., 0.]
        } else {
            [1., 0., 0.]
        },
        n,
    ));
    let v = cross(u, n);
    let width = o.radius.max(1e-9) * 8.;
    let origin = sub(o.center, mul(n, dot(sub(o.center, at), n)));
    let mut slab = Object::cube("saw plane".into(), surface.color);
    for p in &mut slab.p {
        *p = add(
            origin,
            mul(add(mul(u, p[0]), add(mul(n, p[1]), mul(v, -p[2]))), width),
        )
    }
    for f in &mut slab.f {
        f.mat = surface.mat.clone()
    }
    slab.refresh();
    let a = crate::csg::boolean(o, &slab, "intersect")?;
    let b = crate::csg::boolean(o, &slab, "subtract")?;
    if a.f.is_empty() || b.f.is_empty() {
        return Err("plane misses the solid".into());
    }
    Ok([a, b])
}
fn saw_contour(o: &Object, at: V, n: V, surface: &Face) -> Result<[Object; 2], String> {
    if len(n) < 1e-10 {
        return Err("degenerate cutting plane".into());
    }
    let mut halves = [o.clone(), o.clone()];
    for (side, half) in halves.iter_mut().enumerate() {
        let normal = mul(norm(n), if side == 0 { 1. } else { -1. });
        let mut faces = vec![];
        let mut boundary = BTreeSet::new();
        for f in &o.f {
            let v = clipped(half, f, at, normal);
            if v.len() < 3 {
                continue;
            }
            let poly = Face { v, ..f.clone() };
            for e in edges(&poly) {
                if e.iter()
                    .all(|&i| dot(sub(half.p[i], at), normal).abs() < 1e-8 * o.radius.max(1.))
                    && !boundary.insert(e)
                {
                    boundary.remove(&e);
                }
            }
            faces.push(poly)
        }
        half.f = faces;
        let cap = ordered(&boundary).map_err(|_| "cut must cross a single closed contour")?;
        push_face(half, cap, surface);
        half.refresh();
        if dot(half.f.last().unwrap().normal, normal) > 0. {
            half.f.last_mut().unwrap().v.reverse()
        }
        compact(half);
        half.refresh();
    }
    Ok(halves)
}
pub(crate) fn compact(o: &mut Object) {
    let used: BTreeSet<_> =
        o.f.iter()
            .flat_map(|f| f.v.iter().copied())
            .chain(o.wires.iter().flatten().copied())
            .collect();
    let map: BTreeMap<_, _> = used.iter().enumerate().map(|(i, &v)| (v, i)).collect();
    o.p = used.iter().map(|&i| o.p[i]).collect();
    for f in &mut o.f {
        for v in &mut f.v {
            *v = map[v]
        }
    }
    for e in &mut o.wires {
        *e = e.map(|v| map[&v])
    }
}
pub fn mirror(
    o: &mut Object,
    sel: &Selection,
    axis: usize,
    sign: f64,
    shift: f64,
) -> Result<(), String> {
    let mut ids = sel.faces();
    let line_ids: Vec<_> = sel
        .parts
        .iter()
        .filter_map(|p| {
            if let Part::Line(e) = p {
                Some(*e)
            } else {
                None
            }
        })
        .collect();
    if ids.is_empty() && !line_ids.is_empty() {
        let extent = line_ids
            .iter()
            .flatten()
            .map(|&i| sign * o.p[i][axis])
            .fold(f64::NEG_INFINITY, f64::max);
        let plane = sign * extent + shift;
        for e in line_ids {
            let mirrored = e.map(|i| {
                let mut p = o.p[i];
                p[axis] = 2. * plane - p[axis];
                weld(o, p)
            });
            if mirrored != e && !o.wires.contains(&mirrored) {
                o.wires.push(mirrored)
            }
        }
        o.refresh();
        return Ok(());
    }
    if ids.is_empty() {
        ids = (0..o.f.len()).collect()
    }
    let extent = ids
        .iter()
        .flat_map(|&i| o.f[i].v.iter())
        .map(|&i| sign * o.p[i][axis])
        .fold(f64::NEG_INFINITY, f64::max);
    let plane = sign * extent + shift;
    let mut at = [0.; 3];
    at[axis] = plane;
    let mut n = [0.; 3];
    n[axis] = -sign;
    let chosen: BTreeSet<_> = ids.into_iter().collect();
    let old = std::mem::take(&mut o.f);
    for (i, f) in old.into_iter().enumerate() {
        if !chosen.contains(&i) {
            o.f.push(f);
            continue;
        }
        let v = clipped(o, &f, at, n);
        if v.len() < 3 || v.iter().all(|&i| (o.p[i][axis] - plane).abs() < 1e-9) {
            continue;
        }
        let mirrored: Vec<usize> = v
            .iter()
            .rev()
            .map(|&i| {
                let mut p = o.p[i];
                p[axis] = 2. * plane - p[axis];
                weld(o, p)
            })
            .collect();
        if f.normal[axis].abs() < 1e-9 {
            if let Some(k) = (0..v.len()).find(|&k| {
                [v[k], v[(k + 1) % v.len()]]
                    .iter()
                    .all(|&i| (o.p[i][axis] - plane).abs() < 1e-9)
            }) {
                let a = v[k];
                let b = v[(k + 1) % v.len()];
                let j = mirrored.iter().position(|&i| i == a).unwrap();
                let mut outline: Vec<_> = (1..=v.len())
                    .map(|offset| v[(k + offset) % v.len()])
                    .collect();
                for offset in 1..mirrored.len() {
                    let point = mirrored[(j + offset) % mirrored.len()];
                    if point == b {
                        break;
                    }
                    outline.push(point)
                }
                push_face(o, outline, &f);
                continue;
            }
        }
        push_face(o, v, &f);
        push_face(o, mirrored, &f)
    }
    compact(o);
    o.refresh();
    Ok(())
}
pub fn loop_cut(
    o: &mut Object,
    start: Edge,
    count: usize,
    slide: f64,
) -> Result<Selection, String> {
    if !(1..=128).contains(&count) {
        return Err("choose 1–128 cuts".into());
    }
    let faces = quad_ring(o, start);
    if faces.is_empty() {
        return Err("select an edge in a quad ring".into());
    }
    let mut selection = Selection {
        mode: 2,
        ..Selection::default()
    };
    let mut cache = BTreeMap::new();
    for (i, j) in faces {
        let f = o.f[i].clone();
        let [a, b, c, d] = std::array::from_fn(|k| f.v[(j + k) % 4]);
        let mut prev = [a, d];
        for k in 1..=count {
            let mut pair = [0; 2];
            for (l, (x, y)) in [(a, b), (d, c)].into_iter().enumerate() {
                let key = (edge(x, y), if x < y { k } else { count + 1 - k });
                pair[l] = *cache.entry(key).or_insert_with(|| {
                    let t = (k as f64 / (count + 1) as f64
                        + slide.clamp(-0.45, 0.45) / (count as f64))
                        .clamp(0.001, 0.999);
                    o.p.push(add(o.p[x], mul(sub(o.p[y], o.p[x]), t)));
                    o.p.len() - 1
                });
            }
            push_face(o, vec![prev[0], pair[0], pair[1], prev[1]], &f);
            selection.parts.insert(Part::Line(edge(pair[0], pair[1])));
            prev = pair;
        }
        o.f[i].v = vec![prev[0], b, c, prev[1]]
    }
    o.refresh();
    Ok(selection)
}
pub use crate::bevel::bevel;
/// Only concave faces need cached ear indices. Convex faces render as a fan without allocating.
pub fn concave_triangles(verts: &[usize], points: &[V], normal: V) -> Vec<[usize; 3]> {
    if verts.len() < 4 {
        return vec![];
    }
    let axis = (0..3)
        .max_by(|&i, &j| normal[i].abs().total_cmp(&normal[j].abs()))
        .unwrap();
    let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
    let turn = |a: usize, b: usize, c: usize| {
        let (p, q) = (sub(points[b], points[a]), sub(points[c], points[a]));
        (p[u] * q[v] - p[v] * q[u]) * normal[axis].signum()
    };
    if (0..verts.len()).all(|i| {
        turn(
            verts[i],
            verts[(i + 1) % verts.len()],
            verts[(i + 2) % verts.len()],
        ) >= -1e-14
    }) {
        return vec![];
    }
    let mut rest = verts.to_vec();
    let mut tris = vec![];
    while rest.len() > 3 {
        let n = rest.len();
        let mut ear = None;
        for i in 0..n {
            let [a, b, c] = [rest[(i + n - 1) % n], rest[i], rest[(i + 1) % n]];
            if turn(a, b, c) > 1e-14
                && !rest.iter().any(|&p| {
                    p != a
                        && p != b
                        && p != c
                        && turn(a, b, p) >= 0.
                        && turn(b, c, p) >= 0.
                        && turn(c, a, p) >= 0.
                })
            {
                ear = Some((i, [a, b, c]));
                break;
            }
        }
        let Some((i, tri)) = ear else { return vec![] };
        tris.push(tri);
        rest.remove(i);
    }
    tris.push([rest[0], rest[1], rest[2]]);
    tris
}
pub fn quad_ring(o: &Object, start: Edge) -> BTreeMap<usize, usize> {
    let mut pending = vec![start];
    let mut faces = BTreeMap::new();
    while let Some(e) = pending.pop() {
        for (i, f) in o.f.iter().enumerate() {
            if f.v.len() != 4 || faces.contains_key(&i) {
                continue;
            }
            if let Some(j) = edges(f).position(|a| a == e) {
                faces.insert(i, j);
                pending.push(edge(f.v[(j + 2) % 4], f.v[(j + 3) % 4]));
            }
        }
    }
    faces
}
pub fn parts(o: Object) -> Vec<Object> {
    let mut by_vertex = vec![vec![]; o.p.len()];
    for (i, f) in o.f.iter().enumerate() {
        for &v in &f.v {
            by_vertex[v].push(i)
        }
    }
    let mut seen = vec![false; o.f.len()];
    let mut out = vec![];
    for i in 0..seen.len() {
        if seen[i] {
            continue;
        }
        let mut group = vec![];
        let mut queue = vec![i];
        seen[i] = true;
        while let Some(i) = queue.pop() {
            group.push(i);
            for &j in o.f[i].v.iter().flat_map(|&v| &by_vertex[v]) {
                if !seen[j] {
                    seen[j] = true;
                    queue.push(j);
                }
            }
        }
        let mut part = o.clone();
        part.f = group.iter().map(|&i| o.f[i].clone()).collect();
        part.wires.clear();
        compact(&mut part);
        part.refresh();
        out.push(part)
    }
    out
}
