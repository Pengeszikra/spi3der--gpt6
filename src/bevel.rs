// Port of the reference engine's closed-vertex fan bevel. The two incident faces
// take the same offset at each end, including at corners where selected edges meet.
use crate::{
    geometry::{compact, edge, Part, Selection},
    scene::*,
};
use std::collections::{BTreeMap, BTreeSet};
#[derive(Default)]
struct Fan {
    neighbors: Vec<usize>,
    faces: Vec<usize>,
    chosen: Vec<bool>,
    offset: Vec<V>,
    corners: Vec<usize>,
}
fn normal(points: &[V], ring: &[usize]) -> V {
    let mut n = [0.; 3];
    for (i, &a) in ring.iter().enumerate() {
        let b = ring[(i + 1) % ring.len()];
        n = add(
            n,
            cross(
                sub(points[a], points[ring[0]]),
                sub(points[b], points[ring[0]]),
            ),
        )
    }
    n
}
fn dedup(ring: &mut Vec<usize>) {
    ring.dedup();
    while ring.len() > 1 && ring.first() == ring.last() {
        ring.pop();
    }
}
fn despike(ring: &mut Vec<usize>) {
    dedup(ring);
    while ring.len() > 2 {
        let n = ring.len();
        let Some(i) = (0..n).find(|&i| ring[(i + n - 1) % n] == ring[(i + 1) % n]) else {
            break;
        };
        ring.remove(i);
        ring.remove(i % ring.len());
        dedup(ring)
    }
}
fn add_face(out: &mut Vec<Face>, points: &[V], mut ring: Vec<usize>, want: V, source: &Face) {
    dedup(&mut ring);
    if ring.iter().collect::<BTreeSet<_>>().len() < 3 {
        return;
    }
    if dot(normal(points, &ring), want) < 0. {
        ring.reverse()
    }
    out.push(Face {
        v: ring,
        ..source.clone()
    })
}
pub fn bevel(o: &mut Object, sel: &Selection, width: f64, segments: usize) -> Result<(), String> {
    if width <= 0. || !(1..=32).contains(&segments) {
        return Err("positive width and 1–32 segments required".into());
    }
    let mut half = BTreeMap::new();
    for (i, f) in o.f.iter().enumerate() {
        for (k, &a) in f.v.iter().enumerate() {
            half.insert((a, f.v[(k + 1) % f.v.len()]), i);
        }
    }
    let lines: BTreeSet<_> = sel
        .parts
        .iter()
        .filter_map(|p| match *p {
            Part::Line([a, b]) if half.contains_key(&(a, b)) && half.contains_key(&(b, a)) => {
                Some(edge(a, b))
            }
            _ => None,
        })
        .collect();
    if lines.is_empty() {
        return Err("select an edge with two faces".into());
    }
    let mut fans = BTreeMap::new();
    let mut limit = width;
    for v in lines.iter().flatten().copied().collect::<BTreeSet<_>>() {
        let mut next = BTreeMap::new();
        for (i, f) in o.f.iter().enumerate() {
            if let Some(k) = f.v.iter().position(|&p| p == v) {
                let n = f.v.len();
                next.insert(f.v[(k + n - 1) % n], (i, f.v[(k + 1) % n]));
            }
        }
        let start = *next.keys().next().ok_or("empty fan")?;
        let mut p = start;
        let mut fan = Fan::default();
        loop {
            let (i, q) = next.remove(&p).ok_or("bevel needs a closed vertex fan")?;
            fan.faces.push(i);
            fan.neighbors.push(p);
            fan.chosen.push(lines.contains(&edge(v, p)));
            p = q;
            if p == start {
                break;
            }
        }
        if !next.is_empty() {
            return Err("disconnected vertex fan".into());
        }
        let n = fan.faces.len();
        for i in 0..n {
            let j = (i + 1) % n;
            let a = sub(o.p[fan.neighbors[i]], o.p[v]);
            let b = sub(o.p[fan.neighbors[j]], o.p[v]);
            let (u, t) = (norm(a), norm(b));
            let sin = len(cross(u, t));
            let (l, r) = (fan.chosen[i], fan.chosen[j]);
            let mut off = [0.; 3];
            if l || r {
                if sin < 0.001 {
                    return Err("straight corner at bevel endpoint".into());
                }
                if l {
                    limit = limit.min(0.45 * sin * len(b));
                    off = add(off, t)
                }
                if r {
                    limit = limit.min(0.45 * sin * len(a));
                    off = add(off, u)
                }
                off = mul(off, 1. / sin)
            }
            fan.offset.push(off)
        }
        fans.insert(v, fan);
    }
    let mut points = o.p.clone();
    let size = o.radius.max(1e-9);
    let mut registry = BTreeMap::new();
    fn put(
        points: &mut Vec<V>,
        registry: &mut BTreeMap<[i64; 3], usize>,
        p: V,
        center: V,
        size: f64,
    ) -> usize {
        let key = sub(p, center).map(|c| (c / (size * 1e-9)).round() as i64);
        *registry.entry(key).or_insert_with(|| {
            points.push(p);
            points.len() - 1
        })
    }
    for (&v, fan) in &mut fans {
        for &off in &fan.offset {
            let i = if off == [0.; 3] {
                v
            } else {
                put(
                    &mut points,
                    &mut registry,
                    add(o.p[v], mul(off, limit)),
                    o.center,
                    size,
                )
            };
            fan.corners.push(i)
        }
    }
    let mut profiles = BTreeMap::new();
    for (&v, fan) in &fans {
        let n = fan.faces.len();
        for (i, &w) in fan.neighbors.iter().enumerate() {
            if !fan.chosen[i] {
                continue;
            }
            let a = fan.corners[(i + n - 1) % n];
            let b = fan.corners[i];
            let mut chain = vec![a];
            for j in 1..segments {
                let t = j as f64 / segments as f64;
                let p = add(
                    add(
                        mul(points[a], (1. - t).powi(2)),
                        mul(o.p[v], 2. * t * (1. - t)),
                    ),
                    mul(points[b], t * t),
                );
                chain.push(put(&mut points, &mut registry, p, o.center, size))
            }
            chain.push(b);
            profiles.insert((v, w), chain);
        }
    }
    let mut extra = vec![];
    for [a, b] in lines {
        let (p, q) = (&profiles[&(a, b)], &profiles[&(b, a)]);
        let src = &o.f[half[&(a, b)]];
        let want = add(src.normal, o.f[half[&(b, a)]].normal);
        for j in 0..segments {
            add_face(
                &mut extra,
                &points,
                vec![p[j], p[j + 1], q[segments - j - 1], q[segments - j]],
                want,
                src,
            )
        }
    }
    let mut replace = BTreeMap::new();
    for (&v, fan) in &fans {
        let n = fan.faces.len();
        let mut ring = vec![];
        let mut untouched = vec![];
        for i in 0..n {
            let j = (i + 1) % n;
            ring.push(fan.corners[i]);
            if fan.chosen[j] {
                ring.extend_from_slice(&profiles[&(v, fan.neighbors[j])][1..segments])
            }
            if !fan.chosen[i] && !fan.chosen[j] {
                untouched.push(fan.faces[i])
            }
        }
        despike(&mut ring);
        if untouched.len() == 1 {
            if let Some(k) = ring.iter().position(|&i| i == v) {
                ring.rotate_left(k);
                ring.remove(0);
                ring.reverse();
                replace.insert((untouched[0], v), ring);
                continue;
            }
        }
        let want = fan
            .faces
            .iter()
            .fold([0.; 3], |n, &i| add(n, o.f[i].normal));
        add_face(&mut extra, &points, ring, want, &o.f[fan.faces[0]])
    }
    let mut result = o.f.clone();
    for (i, f) in result.iter_mut().enumerate() {
        let mut ring = vec![];
        for &v in &f.v {
            let Some(fan) = fans.get(&v) else {
                ring.push(v);
                continue;
            };
            if let Some(r) = replace.get(&(i, v)) {
                ring.extend_from_slice(r);
                continue;
            }
            let n = fan.faces.len();
            let k = fan.faces.iter().position(|&j| j == i).unwrap();
            let mine = fan.corners[k];
            let before = fan.corners[(k + n - 1) % n];
            let j = (k + 1) % n;
            let after = fan.corners[j];
            let dist = |p| len(sub(points[p], o.p[v]));
            if !fan.chosen[k] && dist(before) > dist(mine) + 1e-12 {
                ring.push(before)
            }
            ring.push(mine);
            if !fan.chosen[j] && dist(after) > dist(mine) + 1e-12 {
                ring.push(after)
            }
        }
        dedup(&mut ring);
        f.v = ring;
    }
    result.extend(extra);
    o.p = points;
    o.f = result;
    compact(o);
    o.refresh();
    Ok(())
}
