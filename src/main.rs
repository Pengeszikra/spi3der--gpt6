mod bench;
mod bevel;
mod camera;
mod csg;
mod edit;
mod export;
mod geometry;
mod session;
mod terminal;
mod ui;
use camera::*;
use geometry::*;
use json::J;
use session::Session;
mod highlight;
mod json;
mod mcp;
mod md;
mod render;
mod scene;
mod store;
use render::*;
use scene::*;
use std::{
    fs,
    io::{self, IsTerminal, Read, Write},
    time::Instant,
};
const VERSION: &str = env!("CARGO_PKG_VERSION");
const BUILD: &str = concat!(env!("CARGO_PKG_VERSION"), " ", env!("BUILD_HASH"));
const HELP: &str = include_str!("../help/index.md");
fn load(n: &str) -> Result<Scene, String> {
    let p = store::resolve(n)?;
    Scene::read(&fs::read_to_string(p).map_err(|e| e.to_string())?)
}
struct App {
    scene: Scene,
    view: View,
    target: Option<usize>,
    face: Option<usize>,
    edit: bool,
    undo: Vec<Scene>,
    generation: u64,
    flight: Flight,
    sel: Selection,
    material: String,
    operands: Vec<usize>,
    undo_floor: usize,
    size: (usize, usize),
    tab: Vec<usize>,
    note: String,
    shared: Option<std::path::PathBuf>,
}
impl App {
    fn new(scene: Scene) -> Self {
        let view = View::home(&scene, 80, 48, 1.);
        Self {
            view,
            scene,
            flight: Flight::default(),
            sel: Selection {
                mode: 3,
                ..Selection::default()
            },
            material: "plaster".into(),
            operands: vec![],
            undo_floor: 0,
            size: (80, 48),
            generation: 0,
            tab: vec![],
            target: None,
            face: None,
            edit: false,
            undo: vec![],
            note: String::new(),
            shared: None,
        }
    }
    fn reframe(&mut self) {
        self.view = View::home(&self.scene, self.size.0, self.size.1, self.view.aspect);
        self.flight.stop();
        self.operands.clear();
        self.tab.clear();
    }
    fn aim(&mut self, i: usize) {
        if let Some(o) = self.scene.objects.get(i) {
            if self.scene.sky {
                self.view.free();
                self.flight.steer = true;
                self.flight.arming = None;
                self.flight.orbit = false;
                self.flight.galaxy_brake = false;
            } else {
                self.view.aim(o.center);
            }
            self.target = Some(i);
            self.face = None;
            self.sel.parts.clear();
        }
    }
    fn target_order(&self) -> Vec<usize> {
        let cam = self.view.camera(self.size.0, self.size.1);
        let mut ranks: Vec<_> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .map(|(i, o)| {
                let q = cam.project(o.center);
                let visible = q[2] > 0. && q[0] >= 0. && q[1] >= 0. && q[0] < cam.w && q[1] < cam.h;
                let score = if visible {
                    (q[0] - cam.w / 2.).hypot(q[1] - cam.h / 2.)
                } else {
                    len(sub(o.center, cam.eye))
                };
                (!visible, score, i)
            })
            .collect();
        ranks.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)).then(a.2.cmp(&b.2)));
        ranks.into_iter().map(|v| v.2).collect()
    }
    fn cycle(&mut self, reverse: bool) {
        if self.tab.is_empty() {
            self.tab = self.target_order()
        }
        let n = self.tab.len();
        if n == 0 {
            return;
        }
        let i = self
            .target
            .and_then(|t| self.tab.iter().position(|&i| i == t));
        let k = if reverse {
            i.map_or(n - 1, |i| (i + n - 1) % n)
        } else {
            i.map_or(0, |i| (i + 1) % n)
        };
        self.aim(self.tab[k])
    }
    fn reload(&mut self, scene: Scene) {
        let old = self
            .target
            .and_then(|i| self.scene.objects.get(i))
            .map(|o| (o.name.clone(), o.original.get("id").clone()));
        self.scene = scene;
        self.target = old.and_then(|(name, id)| {
            self.scene.objects.iter().position(|o| {
                if id.is_null() {
                    o.name == name
                } else {
                    o.original.get("id") == &id
                }
            })
        });
        self.face = None;
        self.edit = false;
        self.sel.parts.clear();
        self.tab.clear();
        self.operands.clear();
        self.undo_floor = 0;
        self.undo.clear();
        self.note = "scene reloaded".into()
    }
    fn surface(&self) -> Face {
        let materials = self.scene.materials();
        let m = materials
            .iter()
            .find(|m| m.get("name").str() == self.material);
        Face {
            mat: self.material.clone(),
            color: color(m.map_or("#c8c8c8", |m| m.get("color").str())),
            emit: m.map_or(0., |m| m.get("emit").num()),
            ..Face::default()
        }
    }
    fn snapshot(&mut self) {
        self.generation = self.generation.wrapping_add(1);
        self.tab.clear();
        if self.undo.len() == 32 {
            self.undo.remove(0);
            self.undo_floor = self.undo_floor.saturating_sub(1);
        }
        self.undo.push(self.scene.clone())
    }
    fn export(&self, p: &std::path::Path) -> Result<(), String> {
        if self
            .shared
            .as_ref()
            .is_some_and(|active| p.canonicalize().ok().as_ref() == Some(active))
        {
            return Ok(());
        }
        store::atomic(p, &self.scene.write()).map_err(|e| e.to_string())
    }
    fn active(&self) -> Result<usize, String> {
        self.target
            .filter(|&i| i < self.scene.objects.len())
            .ok_or("select an object first".into())
    }
    fn command(&mut self, line: &str) -> Result<bool, String> {
        let line = line.trim();
        let (cmd, arg) = line.split_once(' ').unwrap_or((line, ""));
        let arg = arg.trim();
        let words: Vec<_> = arg.split_whitespace().collect();
        match cmd {
            "/q" | "/quit" => return Ok(false),
            "/png" | "/svg" | "/gif" | "/anim" => {
                let animated = matches!(cmd, "/gif" | "/anim");
                let size = if animated {
                    self.size.0
                } else {
                    words
                        .first()
                        .map(|s| s.parse::<usize>().map_err(|_| "expected pixel width"))
                        .transpose()?
                        .unwrap_or(if cmd == "/png" { 1200 } else { 800 })
                };
                if !(8..=2048).contains(&size) {
                    return Err("width must be 8–2048 pixels".into());
                }
                let h = (size as f64 * self.size.1 as f64 / self.size.0 as f64 * self.view.aspect)
                    .round()
                    .clamp(1., 2048.) as usize;
                let frames = if animated {
                    words
                        .first()
                        .map(|s| s.parse::<usize>().map_err(|_| "invalid frame count"))
                        .transpose()?
                        .unwrap_or(36)
                } else {
                    1
                };
                let seconds = words
                    .get(1)
                    .map(|s| s.parse::<f64>().map_err(|_| "invalid seconds"))
                    .transpose()?
                    .unwrap_or(3.);
                if !(1..=240).contains(&frames) || !seconds.is_finite() || seconds <= 0. {
                    return Err("expected 1–240 frames and positive seconds".into());
                }
                let mut view = self.view.clone();
                view.aspect = 1.;
                let ext = if cmd == "/anim" { "svg" } else { &cmd[1..] };
                let stamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_nanos();
                let p = store::root()
                    .join("exports")
                    .join(format!("view-{stamp}.{ext}"));
                let bytes = match cmd {
                    "/png" => export::png(&render(&self.scene, &view, size, h, None, None)),
                    "/gif" => export::gif(&self.scene, &view, size, h, frames, seconds),
                    _ => export::svg(&self.scene, &view, size, h, frames, seconds).into_bytes(),
                };
                store::atomic_bytes(&p, &bytes).map_err(|e| e.to_string())?;
                self.note = p.display().to_string();
            }
            "/spin" => self.flight.spin = switch(arg, self.flight.spin)?,
            "/edges" | "/outline" => self.view.edges = switch(arg, self.view.edges)?,
            "/freeze" => self.flight.frozen = switch(arg, self.flight.frozen)?,
            "/mat" => {
                if !self
                    .scene
                    .materials()
                    .iter()
                    .any(|m| m.get("name").str() == arg)
                {
                    return Err("unknown material".into());
                }
                self.material = arg.into()
            }
            "/go" => {
                if words.len() == 3
                    && words
                        .iter()
                        .all(|s| s.parse::<f64>().is_ok_and(f64::is_finite))
                {
                    let p = std::array::from_fn(|i| words[i].parse().unwrap());
                    self.target = None;
                    self.view.free();
                    self.view.center = p;
                } else {
                    let i = self.find(arg).first().copied().ok_or("target not found")?;
                    self.aim(i);
                }
            }
            "/bool" => {
                let [a, b] = self.operands.as_slice() else {
                    return Err("B picks two objects".into());
                };
                let (a, b) = (*a, *b);
                let lhs = self.scene.objects.get(a).ok_or("first operand is gone")?;
                let rhs = self.scene.objects.get(b).ok_or("second operand is gone")?;
                let result = if arg == "subtract-swap" {
                    csg::boolean(rhs, lhs, "subtract")?
                } else {
                    csg::boolean(lhs, rhs, arg)?
                };
                self.snapshot();
                self.scene.objects.remove(a.max(b));
                self.scene.objects.remove(a.min(b));
                self.scene.objects.push(result);
                self.target = Some(self.scene.objects.len() - 1);
                self.operands.clear();
                self.face = None;
                self.sel.parts.clear();
            }
            "/ver" => self.note = BUILD.into(),
            "/add" => {
                let i = self.scene.objects.len();
                let mut o = primitive(arg, &format!("{} {}", arg, i + 1), &self.surface())?;
                for p in &mut o.p {
                    p[0] += i as f64 * 1.2
                }
                o.refresh();
                self.snapshot();
                self.scene.objects.push(o);
                self.sel.parts.clear();
                self.face = None;
                self.target = Some(i);
            }
            "/new" => {
                self.snapshot();
                self.scene = Scene::default();
                self.reframe();
                self.sel.parts.clear();
                self.target = None;
                self.face = None;
                self.edit = false
            }
            "/l" | "/load" | "/open" | "/merge" => {
                if arg.is_empty() {
                    self.note = store::catalog()
                        .into_iter()
                        .map(|(name, _)| name)
                        .collect::<Vec<_>>()
                        .join(" | ");
                    return Ok(true);
                }
                let s = load(arg)?;
                if matches!(cmd, "/merge" | "/open") {
                    self.merge(s)?
                } else {
                    self.snapshot();
                    self.scene = s;
                    self.reframe();
                    self.sel.parts.clear();
                    self.target = None;
                    self.face = None;
                    self.edit = false
                }
            }
            "/s" | "/save" => {
                let default_name = format!(
                    "scene-{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_millis()
                );
                let arg = if arg.is_empty() { &default_name } else { arg };
                let p = store::path(arg).map_err(|e| e.to_string())?;
                self.export(&p)?;
                self.note = format!("saved {}", p.display())
            }
            "/dl" => {
                if arg.is_empty() {
                    return Err("/dl file.3draft".into());
                }
                self.export(std::path::Path::new(arg))?;
                self.note = format!("saved {arg}")
            }
            "/store" => self.note = store::list().map_err(|e| e.to_string())?.join(" | "),
            "/forget" => {
                fs::remove_file(store::path(arg).map_err(|e| e.to_string())?)
                    .map_err(|e| e.to_string())?;
                self.note = "removed".into()
            }
            "/ls" | "/objects" => {
                self.note = self
                    .scene
                    .objects
                    .iter()
                    .map(|o| o.name.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ")
            }
            "/view" => {
                let (y, p) = match arg {
                    "front" => (0., 0.),
                    "back" => (180., 0.),
                    "left" => (-90., 0.),
                    "right" => (90., 0.),
                    "top" => (0., 90.),
                    "bottom" => (0., -90.),
                    "iso" => (-45., 30.),
                    _ => return Err("unknown view".into()),
                };
                self.view.yaw = y;
                self.view.pitch = p;
                self.view.ortho = arg != "iso" && !self.view.sky;
            }
            "/fit" | "/reset" => self.reframe(),
            "/zoom" => {
                self.view.zoom = arg
                    .parse::<f64>()
                    .map_err(|_| "zoom number")?
                    .clamp(0.05, 100.);
                if !self.view.zoom.is_finite() {
                    self.view.zoom = 1.
                }
            }
            "/face" => {
                let i = self.active()?;
                let f = arg.parse::<usize>().map_err(|_| "face index")?;
                if f >= self.scene.objects[i].f.len() {
                    return Err("face index out of bounds".into());
                }
                self.face = Some(f);
                self.sel.mode = 3;
                self.sel.parts = [Part::Polygon(f)].into();
            }
            "/g" | "/scl" | "/r" => {
                if words.len() != 2 {
                    return Err("axis and value required".into());
                }
                let axis = match words[0] {
                    "x" => 0,
                    "y" => 1,
                    "z" => 2,
                    "all" if cmd == "/scl" => 3,
                    _ => return Err("axis x/y/z".into()),
                };
                let n = words[1].parse::<f64>().map_err(|_| "value")?;
                if !n.is_finite() {
                    return Err("finite value required".into());
                }
                let i = self.active()?;
                let o = &self.scene.objects[i];
                if o.star.is_some() {
                    return Err("cannot transform a sky point as a mesh".into());
                }
                let ids = if self.edit && !self.sel.parts.is_empty() {
                    self.sel.vertices(o)
                } else if self.edit {
                    self.face
                        .and_then(|f| o.f.get(f))
                        .map(|f| f.v.clone())
                        .unwrap_or_else(|| (0..o.p.len()).collect())
                } else {
                    (0..o.p.len()).collect()
                };
                self.snapshot();
                self.scene.objects[i].transform(
                    match cmd {
                        "/g" => "g",
                        "/r" => "r",
                        _ => "s",
                    },
                    axis,
                    n,
                    &ids,
                )
            }
            "/e" => {
                if !self.edit {
                    return Err("enter Q mode first".into());
                }
                let i = self.active()?;
                let f = self.face.ok_or("select face")?;
                let d = arg.parse::<f64>().map_err(|_| "distance")?;
                if !d.is_finite() {
                    return Err("finite distance required".into());
                }
                self.snapshot();
                self.scene.objects[i].extrude(f, d)?
            }
            "/undo" => {
                if self.edit && self.undo.len() <= self.undo_floor {
                    return Ok(true);
                }
                if let Some(s) = self.undo.pop() {
                    self.generation += 1;
                    self.scene = s;
                    self.sel.parts.clear();
                    self.face = None;
                    self.tab.clear();
                    if !self.edit || self.active().is_err() {
                        self.reframe();
                        self.target = None;
                        self.edit = false
                    }
                }
            }
            "/help" | "/keys" => {
                self.note = "Q edit | T target | Enter commands | ? help | Ctrl+C exit".into()
            }
            "" => {}
            _ if !cmd.starts_with('/') => {
                let i = self.find(line).first().copied().ok_or("target not found")?;
                self.aim(i);
                self.tab.clear();
            }
            _ => return Err(format!("unsupported command: {cmd}")),
        }
        Ok(true)
    }
}
fn switch(arg: &str, prev: bool) -> Result<bool, String> {
    match arg {
        "on" => Ok(true),
        "off" => Ok(false),
        "" => Ok(!prev),
        _ => Err("expected on or off".into()),
    }
}
impl App {
    fn find(&self, text: &str) -> Vec<usize> {
        let q = fold(text);
        let eye = self.view.eye();
        let mut hits: Vec<_> = self
            .scene
            .objects
            .iter()
            .enumerate()
            .filter_map(|(i, o)| {
                let n = fold(&o.name);
                let kind = o.star.as_ref().map_or("", |p| &p.kind);
                (n.contains(&q) || fold(kind).contains(&q))
                    .then(|| (n != q, !n.starts_with(&q), len(sub(o.center, eye)), i))
            })
            .collect();
        hits.sort_by(|a, b| {
            a.0.cmp(&b.0)
                .then(a.1.cmp(&b.1))
                .then(a.2.total_cmp(&b.2))
                .then(a.3.cmp(&b.3))
        });
        hits.into_iter().map(|v| v.3).collect()
    }
    fn merge(&mut self, s: Scene) -> Result<(), String> {
        let mut left = self.scene.document();
        let mut right = s.document();
        let mut objs = left.take("objects").arr().to_vec();
        objs.extend(right.take("objects").arr().iter().cloned());
        if let J::Obj(fields) = right {
            for (k, v) in fields {
                match k.as_str() {
                    "format" | "name" => {}
                    "version" => {
                        left.put("version", J::Num(v.num().max(left.get("version").num())))
                    }
                    "palette" => {
                        let mut pal = left.get("palette").arr().to_vec();
                        for m in v.arr() {
                            if let Some(existing) =
                                pal.iter().find(|x| x.get("name") == m.get("name"))
                            {
                                if existing != m {
                                    return Err(format!(
                                        "conflicting material {}",
                                        m.get("name").str()
                                    ));
                                }
                            } else {
                                pal.push(m.clone())
                            }
                        }
                        left.put(&k, J::Arr(pal))
                    }
                    "prototypes" => {
                        let mut p = left.take(&k);
                        if p.is_null() {
                            p = J::object()
                        }
                        if let J::Obj(protos) = v {
                            for (key, def) in protos {
                                if !p.get(&key).is_null() && p.get(&key) != &def {
                                    return Err(format!("conflicting prototype {key}"));
                                }
                                p.put(&key, def)
                            }
                        }
                        left.put(&k, p)
                    }
                    _ => {
                        if left.get(&k).is_null() {
                            left.put(&k, v)
                        } else if left.get(&k) != &v {
                            return Err(format!("conflicting scene field {k}"));
                        }
                    }
                }
            }
        }
        left.put("objects", J::Arr(objs));
        let merged = Scene::read(&left.encode())?;
        self.snapshot();
        self.scene = merged;
        Ok(())
    }
}
fn fold(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .map(|c| match c {
            'á' | 'à' | 'â' => 'a',
            'é' | 'è' | 'ê' => 'e',
            'í' | 'ï' => 'i',
            'ó' | 'ö' | 'ő' | 'ô' => 'o',
            'ú' | 'ü' | 'ű' => 'u',
            v => v,
        })
        .collect()
}
fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let has = |s: &str| args.iter().any(|x| x == s);
    let opt = |s: &str| {
        args.iter()
            .position(|x| x == s)
            .and_then(|i| args.get(i + 1))
    };
    if has("--mcp") {
        return mcp::run(opt("--session").or(opt("--scene")).map(Into::into));
    }
    if has("--version") {
        println!("{BUILD}");
        return Ok(());
    }
    if has("--help") {
        println!("{}", md::ansi(HELP));
        return Ok(());
    }
    if let Some(p) = opt("--markdown").or_else(|| opt("--md")) {
        let s = fs::read_to_string(p).map_err(|e| e.to_string())?;
        let width = opt("--width")
            .or(opt("--cols"))
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or_else(|| terminal::size().0)
            .clamp(8, 1000);
        println!("{}", md::document(&s, p, width).join("\n"));
        return Ok(());
    }
    let scene_arg = opt("--scene").or_else(|| args.first().filter(|s| !s.starts_with('-')));
    let load_start = Instant::now();
    let scene = if let Some(p) = scene_arg {
        load(p)?
    } else if opt("--session").is_some() {
        Scene::default()
    } else {
        Scene {
            objects: vec![Object::cube("Cube".into(), [205, 160, 95])],
            ..Scene::default()
        }
    };
    let load_ms = load_start.elapsed().as_secs_f64() * 1000.;
    let mut a = App::new(scene);
    let w = opt("--cols")
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(80)
        .clamp(10, 400);
    let h = opt("--rows")
        .and_then(|x| x.parse::<usize>().ok())
        .unwrap_or(24)
        .clamp(4, 200);
    a.size = (w, h * 2);
    a.reframe();
    if let Some(p) = opt("--script") {
        for l in fs::read_to_string(p).map_err(|e| e.to_string())?.lines() {
            if !l.trim().is_empty() && !l.starts_with('#') && !a.command(l)? {
                break;
            }
        }
    }
    if let Some(n) = opt("--bench") {
        let n = n
            .parse::<usize>()
            .map_err(|_| "frame count")?
            .clamp(1, 100000);
        return bench::run(&mut a, n, has("--bench-stream"), load_ms);
    }
    if has("--once") || !io::stdin().is_terminal() {
        println!(
            "{}",
            render(&a.scene, &a.view, w, h * 2, a.target, a.face)
                .rows()
                .join("\n")
        );
        return Ok(());
    }
    let path = if let Some(p) = opt("--session") {
        Some(p.into())
    } else if has("--watch") {
        Some(store::resolve(
            opt("--scene").ok_or("--watch needs --scene")?,
        )?)
    } else if scene_arg.is_none() {
        Some(Session::default_path())
    } else {
        None
    };
    let session = if let Some(path) = path {
        let (session, scene) = Session::open(path, &a.scene)?;
        a.shared = session.path.canonicalize().ok();
        a.reload(scene);
        a.reframe();
        Some(session)
    } else {
        None
    };
    ui::run(a, session)
}

#[cfg(test)]
mod round3_tests;
