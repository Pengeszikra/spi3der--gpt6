use crate::json::{parse, J};
use std::{collections::BTreeMap, sync::Arc};
pub type V = [f64; 3];
pub fn sub(a: V, b: V) -> V {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
pub fn cross(a: V, b: V) -> V {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}
pub fn dot(a: V, b: V) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
pub fn norm(a: V) -> V {
    let l = dot(a, a).sqrt().max(1e-15);
    a.map(|x| x / l)
}
pub fn rotate(mut p: V, axis: usize, a: f64) -> V {
    let i = (axis + 1) % 3;
    let j = (axis + 2) % 3;
    let (s, c) = a.sin_cos();
    (p[i], p[j]) = (p[i] * c - p[j] * s, p[i] * s + p[j] * c);
    p
}
pub fn add(a: V, b: V) -> V {
    std::array::from_fn(|i| a[i] + b[i])
}
pub fn mul(a: V, k: f64) -> V {
    a.map(|v| v * k)
}
pub fn len(a: V) -> f64 {
    dot(a, a).sqrt()
}
pub fn vec3(j: &J) -> Option<V> {
    let a = j.arr();
    if a.len() != 3 {
        return None;
    }
    Some([a[0].number()?, a[1].number()?, a[2].number()?])
}
pub fn jvec(v: V) -> J {
    J::Arr(v.map(J::Num).into())
}
#[derive(Clone, Debug, Default)]
pub struct Face {
    pub v: Vec<usize>,
    pub color: [u8; 3],
    pub mat: String,
    pub emit: f64,
    pub normal: V,
    pub mid: V,
    pub source: Option<usize>,
    pub triangles: Vec<[usize; 3]>,
}
#[derive(Clone, Debug, Default)]
pub struct Object {
    pub name: String,
    pub p: Vec<V>,
    pub f: Vec<Face>,
    pub original: Arc<J>,
    pub dirty: bool,
    pub center: V,
    pub radius: f64,
    pub star: Option<Star>,
    pub wires: Vec<[usize; 2]>,
}
#[derive(Clone, Debug)]
pub struct Star {
    pub kind: String,
    pub rgb: [u8; 3],
    pub brightness: f64,
    pub big: bool,
    pub far: bool,
    pub galaxy: bool,
}
#[derive(Clone, Debug, Default)]
pub struct Scene {
    pub objects: Vec<Object>,
    pub root: Arc<J>,
    pub dust: Arc<Vec<V>>,
    pub sky: bool,
}
pub fn bounds<'a>(points: impl Iterator<Item = &'a V> + Clone) -> (V, f64) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in points.clone() {
        for i in 0..3 {
            lo[i] = lo[i].min(p[i]);
            hi[i] = hi[i].max(p[i]);
        }
    }
    if !lo[0].is_finite() {
        return ([0.; 3], 1.);
    }
    let center = mul(add(lo, hi), 0.5);
    let r = points.map(|&p| len(sub(p, center))).fold(0., f64::max);
    (center, r)
}
pub fn color(s: &str) -> [u8; 3] {
    let s = s.strip_prefix('#').unwrap_or(s);
    if s.len() == 6 && s.is_ascii() {
        if let Ok(n) = u32::from_str_radix(s, 16) {
            return [(n >> 16) as u8, (n >> 8) as u8, n as u8];
        }
    }
    [180, 185, 190]
}
impl Object {
    pub fn cube(name: String, c: [u8; 3]) -> Self {
        let mut obj = Self {
            name,
            p: vec![
                [-0.5, 0., -0.5],
                [0.5, 0., -0.5],
                [0.5, 1., -0.5],
                [-0.5, 1., -0.5],
                [-0.5, 0., 0.5],
                [0.5, 0., 0.5],
                [0.5, 1., 0.5],
                [-0.5, 1., 0.5],
            ],
            f: vec![
                [4, 5, 6, 7],
                [1, 0, 3, 2],
                [1, 2, 6, 5],
                [0, 4, 7, 3],
                [3, 7, 6, 2],
                [0, 1, 5, 4],
            ]
            .into_iter()
            .map(|v| Face {
                v: v.to_vec(),
                color: c,
                ..Face::default()
            })
            .collect(),
            dirty: true,
            ..Self::default()
        };
        obj.refresh();
        obj
    }
    pub fn refresh(&mut self) {
        self.dirty = true;
        if self.star.is_some() {
            return;
        }
        (self.center, self.radius) = bounds(self.p.iter());
        for f in &mut self.f {
            let mut n = [0.; 3];
            let mut m = [0.; 3];
            for (i, &v) in f.v.iter().enumerate() {
                let a = self.p[v];
                let b = self.p[f.v[(i + 1) % f.v.len()]];
                m = add(m, a);
                for (j, value) in n.iter_mut().enumerate() {
                    let k = (j + 1) % 3;
                    let l = (j + 2) % 3;
                    *value += (a[k] - b[k]) * (a[l] + b[l]);
                }
            }
            f.mid = mul(m, 1. / f.v.len().max(1) as f64);
            f.normal = norm(n);
            f.triangles = crate::geometry::concave_triangles(&f.v, &self.p, f.normal);
        }
    }
    pub fn transform(&mut self, op: &str, axis: usize, n: f64, ids: &[usize]) {
        let mut c = [0.; 3];
        for &i in ids {
            for j in 0..3 {
                c[j] += self.p[i][j]
            }
        }
        c = c.map(|x| x / ids.len().max(1) as f64);
        for &i in ids {
            let p = &mut self.p[i];
            match op {
                "g" => p[axis] += n,
                "s" => {
                    for j in 0..3 {
                        if j == axis || axis == 3 {
                            p[j] = c[j] + (p[j] - c[j]) * n
                        }
                    }
                }
                "r" => {
                    let q = rotate(sub(*p, c), axis, n.to_radians());
                    *p = [q[0] + c[0], q[1] + c[1], q[2] + c[2]]
                }
                _ => {}
            }
        }
        self.refresh();
    }
    pub fn extrude(&mut self, fi: usize, d: f64) -> Result<(), String> {
        let f = self.f.get(fi).ok_or("face index")?.clone();
        if f.v.len() < 3 {
            return Err("not a polygon".into());
        }
        let n = norm(cross(
            sub(self.p[f.v[1]], self.p[f.v[0]]),
            sub(self.p[f.v[2]], self.p[f.v[0]]),
        ));
        let start = self.p.len();
        for &i in &f.v {
            let p = self.p[i];
            self.p
                .push([p[0] + n[0] * d, p[1] + n[1] * d, p[2] + n[2] * d]);
        }
        let len = f.v.len();
        for i in 0..len {
            let j = (i + 1) % len;
            self.f.push(Face {
                v: vec![f.v[i], f.v[j], start + j, start + i],
                ..f.clone()
            });
        }
        self.f[fi].v = (start..start + len).collect();
        self.refresh();
        Ok(())
    }
}
impl Scene {
    pub fn materials(&self) -> Vec<J> {
        let mut materials = parse(include_str!("materials.json"))
            .unwrap()
            .arr()
            .to_owned();
        for m in self.root.get("palette").arr() {
            if let Some(old) = materials
                .iter_mut()
                .find(|v| v.get("name") == m.get("name"))
            {
                *old = m.clone()
            } else {
                materials.push(m.clone())
            }
        }
        materials
    }
    pub fn read(s: &str) -> Result<Self, String> {
        let mut root = parse(s)?;
        if root.get("format").str() != "3draft-scene" {
            return Err("expected 3draft-scene".into());
        }
        let J::Arr(raw) = root.take("objects") else {
            return Err("expected objects array".into());
        };
        let mut scene = Scene {
            sky: root.get("vehicle").str() == "starship"
                || (raw.len() > 1500 && !root.get("prototypes").is_null()),
            root: Arc::new(root),
            ..Scene::default()
        };
        let materials = scene.materials();
        let palette: BTreeMap<_, _> = materials.iter().map(|m| (m.get("name").str(), m)).collect();
        let eye = vec3(scene.root.get("start").get("eye")).unwrap_or_default();
        let mut groups: BTreeMap<String, Vec<(usize, f64)>> = BTreeMap::new();
        for raw in raw {
            let mut o = Object {
                name: raw.get("name").str().into(),
                ..Object::default()
            };
            let proto = raw.get("proto").str();
            let at = vec3(raw.get("at"));
            if scene.sky && at.is_some() && raw.get("mesh").is_null() {
                o.center = at.unwrap_or_default();
                o.radius = raw
                    .get("scale")
                    .number()
                    .or(raw.get("scale").get("core").number())
                    .unwrap_or(1.);
                let kind = raw.get("category").str();
                let kind = if kind.is_empty() { proto } else { kind };
                let color_key = raw.get("mats").get("core").str();
                let col = [
                    raw.get("glowColor").str(),
                    palette.get(color_key).map_or("", |m| m.get("color").str()),
                    raw.get("color").str(),
                ]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or("#c8c8c8");
                let log = (raw.get("glow").number().unwrap_or(o.radius).max(1e-30)
                    / len(sub(o.center, eye)).max(1e-12))
                .log10();
                groups
                    .entry(kind.into())
                    .or_default()
                    .push((scene.objects.len(), log));
                o.star = Some(Star {
                    kind: kind.into(),
                    far: raw.get("far") == &J::Bool(true),
                    galaxy: proto == "ghost" || !raw.get("galaxy").is_null(),
                    rgb: color(col),
                    brightness: 1.,
                    big: proto == "ghost" || !raw.get("galaxy").is_null(),
                });
            } else {
                let inst = raw.get("mesh").is_null() && !proto.is_empty();
                let mesh = if inst {
                    scene
                        .root
                        .get("prototypes")
                        .get(proto)
                        .get("lods")
                        .arr()
                        .first()
                        .ok_or_else(|| format!("missing prototype {proto}"))?
                } else {
                    raw.get("mesh")
                };
                if !matches!(mesh.get("positions"), J::Arr(_))
                    || !matches!(mesh.get("faces"), J::Arr(_))
                {
                    return Err(format!("invalid mesh on {}", o.name));
                }
                o.p = mesh
                    .get("positions")
                    .arr()
                    .iter()
                    .map(|v| vec3(v).ok_or("invalid vertex".to_owned()))
                    .collect::<Result<_, _>>()?;
                let mut slots = vec![None; o.p.len()];
                for (fi, f) in mesh.get("faces").arr().iter().enumerate() {
                    let mut face = Face {
                        source: Some(fi),
                        ..Face::default()
                    };
                    let slot = f.get("mat").str();
                    let remap = raw.get("mats").get(slot).str();
                    face.mat = if inst && !remap.is_empty() {
                        remap
                    } else {
                        slot
                    }
                    .into();
                    for j in f.get("v").arr() {
                        let n = j.number().ok_or("invalid face index")?;
                        if n < 0. || n.fract() != 0. || n >= o.p.len() as f64 {
                            return Err("invalid face index".into());
                        }
                        face.v.push(n as usize);
                        slots[n as usize].get_or_insert(slot);
                    }
                    let material = palette.get(face.mat.as_str());
                    face.color = color(
                        material
                            .map(|m| m.get("color").str())
                            .or_else(|| {
                                (inst && !raw.get("color").str().is_empty())
                                    .then(|| raw.get("color").str())
                            })
                            .unwrap_or(f.get("color").str()),
                    );
                    face.emit = material.map_or(0., |m| m.get("emit").num());
                    o.f.push(face);
                }
                if inst {
                    let rot = raw.get("rot").arr();
                    let rot_x = rot.first().map_or(0., J::num);
                    let rot_y = rot.get(1).map_or(0., J::num);
                    let scale = raw.get("scale");
                    for (p, slot) in o.p.iter_mut().zip(slots) {
                        let k = scale
                            .number()
                            .or(scale.get(slot.unwrap_or("")).number())
                            .or(scale.get("*").number())
                            .unwrap_or(1.);
                        *p = add(
                            at.unwrap_or_default(),
                            rotate(rotate(mul(*p, k), 0, rot_x), 1, rot_y),
                        );
                    }
                }
                for wire in raw.get("wire").arr().iter().chain(mesh.get("lines").arr()) {
                    if let [a, b] = wire.arr() {
                        if let (Some(a), Some(b)) = (a.number(), b.number()) {
                            if a >= 0.
                                && b >= 0.
                                && a.fract() == 0.
                                && b.fract() == 0.
                                && (a as usize) < o.p.len()
                                && (b as usize) < o.p.len()
                            {
                                o.wires.push([a as usize, b as usize]);
                            }
                        }
                    }
                }
                o.refresh();
            }
            o.original = Arc::new(raw);
            o.dirty = false;
            scene.objects.push(o);
        }
        for group in groups.values_mut() {
            group.sort_by(|a, b| a.1.total_cmp(&b.1));
            let lo = group[group.len() * 5 / 100].1;
            let hi = group[(group.len() - 1) * 95 / 100].1;
            for &(i, l) in group.iter() {
                let star = scene.objects[i].star.as_mut().unwrap();
                star.brightness = (0.3 + 0.7 * (l - lo) / (hi - lo).max(1e-9)).clamp(0.28, 1.);
                star.big &= star.brightness > 0.85;
            }
        }
        let cloud = scene.root.get("cloud");
        let unit = cloud.get("unit").number().unwrap_or(1.);
        scene.dust = Arc::new(
            cloud
                .get("p")
                .arr()
                .as_chunks::<3>()
                .0
                .iter()
                .map(|v| mul([v[0].num(), v[1].num(), v[2].num()], unit))
                .collect(),
        );
        Ok(scene)
    }
    pub fn document(&self) -> J {
        let mut root = (*self.root).clone();
        if root.is_null() {
            root = J::object();
            root.put("format", J::Str("3draft-scene".into()));
            root.put("version", J::Num(1.));
            root.put("palette", J::Arr(vec![]));
        }
        let objects = self
            .objects
            .iter()
            .map(|o| {
                if !o.dirty && !o.original.is_null() {
                    return (*o.original).clone();
                }
                let mut raw = (*o.original).clone();
                if raw.is_null() {
                    raw = J::object()
                }
                let proto = raw.get("proto").str();
                let mut mesh = raw.get("mesh").clone();
                if mesh.is_null() {
                    mesh = root
                        .get("prototypes")
                        .get(proto)
                        .get("lods")
                        .arr()
                        .first()
                        .cloned()
                        .unwrap_or_else(J::object);
                }
                if !proto.is_empty() {
                    for k in ["proto", "at", "scale", "rot", "mats"] {
                        raw.take(k);
                    }
                }
                let faces =
                    o.f.iter()
                        .map(|f| {
                            let mut v = f
                                .source
                                .and_then(|i| mesh.get("faces").arr().get(i))
                                .cloned()
                                .unwrap_or_else(J::object);
                            v.put("v", J::Arr(f.v.iter().map(|&i| J::Num(i as f64)).collect()));
                            if !f.mat.is_empty() {
                                v.put("mat", J::Str(f.mat.clone()))
                            }
                            if v.get("color").is_null() || o.original.get("mesh").is_null() {
                                v.put(
                                    "color",
                                    J::Str(format!(
                                        "#{:02x}{:02x}{:02x}",
                                        f.color[0], f.color[1], f.color[2]
                                    )),
                                )
                            }
                            v
                        })
                        .collect();
                mesh.put("positions", J::Arr(o.p.iter().copied().map(jvec).collect()));
                mesh.put("faces", J::Arr(faces));
                if !o.wires.is_empty() || !raw.get("wire").is_null() || !mesh.get("lines").is_null()
                {
                    mesh.take("lines");
                    raw.put(
                        "wire",
                        J::Arr(
                            o.wires
                                .iter()
                                .map(|w| J::Arr(w.iter().map(|&n| J::Num(n as f64)).collect()))
                                .collect(),
                        ),
                    )
                }
                raw.put("name", J::Str(o.name.clone()));
                raw.put("mesh", mesh);
                raw
            })
            .collect();
        root.put("objects", J::Arr(objects));
        root
    }
    pub fn write(&self) -> String {
        self.document().encode()
    }
}
