use crate::{
    edit::{self, Transform},
    terminal::{Key, Terminal},
    *,
};
use std::time::Duration;
#[derive(Default)]
enum Panel {
    #[default]
    Scene,
    Find(String),
    Help(usize),
    Palette,
}
#[derive(Default)]
struct Ui {
    panel: Panel,
    index: usize,
    scroll: usize,
    prop: usize,
    pointer: Option<[usize; 2]>,
    drag: Option<[usize; 2]>,
    moved: bool,
    region: u8,
    path: Vec<[f64; 2]>,
    tool: Option<Transform>,
    draw: char,
    strokes: Vec<V>,
    lathe_count: Option<String>,
    saved_view: Option<View>,
    all: Option<Instant>,
    zen: bool,
    alternate_ring: bool,
    quit: bool,
}
fn safe(s: &str, w: usize) -> String {
    let mut n = 0;
    s.chars()
        .filter(|c| !c.is_control())
        .take_while(|&c| {
            n += md::cells(c);
            n <= w
        })
        .collect()
}
impl Ui {
    fn pos(&self, f: &Frame) -> [usize; 2] {
        self.pointer.unwrap_or([f.w / 2, f.h / 2])
    }
    fn cancel(&mut self, a: &mut App) {
        if let Some(t) = self.tool.take() {
            if let Ok(i) = a.active() {
                a.scene.objects[i] = t.original;
                a.sel = t.selection
            }
        }
        self.region = 0;
        self.path.clear();
        self.draw = '\0';
        self.strokes.clear();
        self.lathe_count = None;
        self.drag = None
    }
    fn choices(&self, a: &App) -> Vec<(String, String)> {
        let Panel::Find(q) = &self.panel else {
            return vec![];
        };
        let (cmd, arg) = q.split_once(' ').unwrap_or((q, ""));
        let arg = arg.trim();
        if matches!(cmd, "/load" | "/l" | "/merge" | "/open") {
            return store::catalog()
                .into_iter()
                .filter(|(n, _)| fold(n).contains(&fold(arg)))
                .map(|(n, _)| (n.clone(), format!("{cmd} {n}")))
                .collect();
        }
        if cmd == "/mat" || cmd == "/add" || cmd == "/view" {
            let names = match cmd {
                "/mat" => a
                    .scene
                    .materials()
                    .iter()
                    .map(|m| m.get("name").str().to_owned())
                    .collect(),
                "/add" => ["box", "prism", "cylinder", "cone", "pyramid", "sphere"]
                    .map(String::from)
                    .to_vec(),
                _ => ["front", "back", "left", "right", "top", "bottom", "iso"]
                    .map(String::from)
                    .to_vec(),
            };
            return names
                .into_iter()
                .filter(|s| s.contains(arg))
                .map(|s| (s.clone(), format!("{cmd} {s}")))
                .collect();
        }
        if q.starts_with('/') {
            return if q == "/" || !q.contains(' ') {
                [
                    "/load", "/merge", "/open", "/save", "/new", "/dl", "/store", "/bool", "/png",
                    "/svg", "/gif", "/anim", "/forget", "/add", "/go", "/mat", "/view", "/fit",
                    "/edges", "/freeze", "/spin", "/zoom", "/help", "/keys", "/undo", "/ver",
                    "/zen", "/quit",
                ]
                .iter()
                .filter(|s| s.starts_with(q))
                .map(|s| (s.to_string(), s.to_string()))
                .collect()
            } else {
                vec![]
            };
        }
        a.find(q)
            .into_iter()
            .map(|i| {
                let o = &a.scene.objects[i];
                (
                    format!("{}  {}", o.name, o.star.as_ref().map_or("", |p| &p.kind)),
                    o.name.clone(),
                )
            })
            .collect()
    }
    fn help(&mut self, q: &str, w: usize) {
        self.panel = Panel::Help(0);
        self.scroll = 0;
        if !q.is_empty() {
            for (i, (_, body)) in md::PAGES.iter().enumerate() {
                if let Some(j) = md::document(body, "md", w)
                    .iter()
                    .position(|l| fold(&md::plain(l)).contains(&fold(q)))
                {
                    self.panel = Panel::Help(i);
                    self.scroll = j;
                    break;
                }
            }
        }
    }
    fn find_key(&mut self, a: &mut App, key: Key) -> Result<(), String> {
        let choices = self.choices(a);
        let Panel::Find(q) = &mut self.panel else {
            return Ok(());
        };
        match key {
            Key::Char(c) => {
                q.push(c);
                self.index = 0
            }
            Key::Back => {
                q.pop();
                self.index = 0
            }
            Key::Esc => self.panel = Panel::Scene,
            Key::Arrow('A') => self.index = self.index.saturating_sub(1),
            Key::Arrow('B') => self.index = (self.index + 1).min(choices.len().saturating_sub(1)),
            Key::Enter => {
                let typed = q.trim().to_owned();
                let (cmd, arg) = typed.split_once(' ').unwrap_or((&typed, ""));
                if matches!(
                    cmd,
                    "/l" | "/load" | "/merge" | "/open" | "/add" | "/mat" | "/view"
                ) && typed == q.trim_end()
                    && !q.ends_with(' ')
                    && arg.is_empty()
                {
                    q.push(' ');
                    self.index = 0;
                    return Ok(());
                }
                let value = choices
                    .get(self.index)
                    .map_or_else(|| typed.clone(), |x| x.1.clone());
                if typed == "/"
                    || (!typed.contains(' ') && typed.starts_with('/') && value != typed)
                {
                    *q = value + " ";
                    self.index = 0;
                    return Ok(());
                }
                self.panel = Panel::Scene;
                if matches!(cmd, "/help" | "/keys") {
                    self.help(arg, a.size.0)
                } else if cmd == "/zen" {
                    self.zen = !self.zen
                } else if !value.is_empty() {
                    self.quit = !a.command(&value)?;
                    if !a.edit {
                        self.saved_view = None;
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }
    fn palette_key(&mut self, a: &mut App, key: Key, h: usize) -> Result<(), String> {
        let mats = a.scene.materials();
        let mut i = mats
            .iter()
            .position(|m| m.get("name").str() == a.material)
            .unwrap_or(0);
        let n = mats.len();
        let keys = ["metal", "rough", "alpha", "emit", "soft"];
        let shown = h.saturating_sub(9).max(1);
        let first = i.saturating_sub(shown / 2).min(n.saturating_sub(shown));
        let mut change = 0.;
        match key {
            Key::Esc | Key::Char('P' | 'p') | Key::Enter => self.panel = Panel::Scene,
            Key::Arrow('A') => i = (i + n - 1) % n,
            Key::Arrow('B') => i = (i + 1) % n,
            Key::Tab(rev) => self.prop = (self.prop + if rev { 4 } else { 1 }) % 5,
            Key::Arrow('C') => change = 0.05,
            Key::Arrow('D') => change = -0.05,
            Key::Mouse {
                code,
                y,
                x,
                up: false,
            } if code & 32 == 0 => {
                let count = shown.min(n);
                if code & 64 != 0 {
                    i = if code & 1 == 0 {
                        (i + n - 1) % n
                    } else {
                        (i + 1) % n
                    }
                } else if (1..=count).contains(&y) {
                    i = first + y - 1
                } else if y > count && y <= count + 5 {
                    self.prop = y - count - 1;
                    let max = if self.prop == 3 { 5. } else { 1. };
                    change = (x.saturating_sub(8) as f64 / 20.).clamp(0., 1.) * max
                        - mats[i].get(keys[self.prop]).num();
                }
            }
            _ => {}
        }
        a.material = mats[i].get("name").str().into();
        if change != 0. {
            let mut m = mats[i].clone();
            let k = keys[self.prop];
            m.put(
                k,
                J::Num(
                    ((m.get(k).num() + change) * 100.)
                        .round()
                        .clamp(0., if k == "emit" { 500. } else { 100. })
                        / 100.,
                ),
            );
            a.snapshot();
            let mut root = (*a.scene.root).clone();
            if root.is_null() {
                root = Scene::default().document();
                root.take("objects");
            }
            let mut pal = root.get("palette").arr().to_vec();
            pal.retain(|p| p.get("name") != m.get("name"));
            pal.push(m.clone());
            root.put("palette", J::Arr(pal));
            a.scene.root = std::sync::Arc::new(root);
            for f in a.scene.objects.iter_mut().flat_map(|o| &mut o.f) {
                if f.mat == a.material {
                    f.emit = m.get("emit").num();
                }
            }
        }
        Ok(())
    }
    fn overlay(&self, a: &App, w: usize, h: usize) -> Option<Vec<String>> {
        match &self.panel {
            Panel::Scene => None,
            Panel::Help(i) => {
                let mut out = vec![safe(
                    &format!(
                        "Help · {}   Tab section   ↑↓ PgUp PgDn   /help word",
                        md::PAGES[*i].0
                    ),
                    w,
                )];
                let page = md::document(md::PAGES[*i].1, "md", w);
                out.extend(
                    page.iter()
                        .skip(self.scroll.min(page.len().saturating_sub(1)))
                        .take(h - 1)
                        .cloned(),
                );
                Some(out)
            }
            Panel::Find(q) => {
                let mut out = vec![safe(&format!("> {q}"), w)];
                let choices = self.choices(a);
                let first = self
                    .index
                    .saturating_sub((h - 2) / 2)
                    .min(choices.len().saturating_sub(h - 1));
                for (i, (name, _)) in choices.iter().enumerate().skip(first).take(h - 1) {
                    let s = safe(
                        &format!("{} {name}", if i == self.index { "▸" } else { " " }),
                        w,
                    );
                    out.push(if i == self.index {
                        format!("\x1b[7m{s}\x1b[0m")
                    } else {
                        s
                    })
                }
                Some(out)
            }
            Panel::Palette => {
                let mats = a.scene.materials();
                let i = mats
                    .iter()
                    .position(|m| m.get("name").str() == a.material)
                    .unwrap_or(0);
                let room = h.saturating_sub(9).max(1);
                let first = i
                    .saturating_sub(room / 2)
                    .min(mats.len().saturating_sub(room));
                let mut out = vec!["Materials".into()];
                for (j, m) in mats.iter().enumerate().skip(first).take(room) {
                    let [r, g, b] = color(m.get("color").str());
                    out.push(format!(
                        "{} \x1b[48;2;{r};{g};{b}m   \x1b[0m {}",
                        if i == j { "▸" } else { " " },
                        safe(m.get("name").str(), w.saturating_sub(6))
                    ))
                }
                for (j, k) in ["metal", "rough", "alpha", "emit", "soft"]
                    .iter()
                    .enumerate()
                {
                    let n = mats[i].get(k).num();
                    out.push(safe(
                        &format!(
                            "{} {k:5} {} {n:.2}",
                            if j == self.prop { "›" } else { " " },
                            "━"
                                .repeat((n / if j == 3 { 5. } else { 1. } * 20.).clamp(0., 20.)
                                    as usize)
                        ),
                        w,
                    ))
                }
                out.push(safe("↑↓ material   Tab property   ←→ value   P close", w));
                Some(out)
            }
        }
    }
    fn decorate(&self, a: &App, f: &mut Frame) {
        let cam = a.view.camera(f.w, f.h);
        if let Some(i) = a.target {
            if let Some(o) = a.scene.objects.get(i) {
                if a.edit {
                    let span = o.radius.max(0.1);
                    let bottom = o.p.iter().map(|p| p[1]).fold(f64::INFINITY, f64::min);
                    let base = [o.center[0], bottom, o.center[2]];
                    for j in -5..=5 {
                        let off = j as f64 * span / 5.;
                        for (p, q) in [
                            ([off, 0., -span], [off, 0., span]),
                            ([-span, 0., off], [span, 0., off]),
                        ] {
                            let (start, end) =
                                (cam.project(add(base, p)), cam.project(add(base, q)));
                            let steps = f.w.max(f.h);
                            for k in 0..=steps {
                                let point =
                                    add(start, mul(sub(end, start), k as f64 / steps as f64));
                                if point[2] > 0.
                                    && point[0] >= 0.
                                    && point[1] >= 0.
                                    && point[0] < f.w as f64
                                    && point[1] < f.h as f64
                                    && f.ids[point[1] as usize * f.w + point[0] as usize].is_none()
                                {
                                    f.mark(point, [57, 64, 70])
                                }
                            }
                        }
                    }
                    for part in a.sel.candidates(o) {
                        let on = a.sel.parts.contains(&part);
                        let c = if on { [255, 180, 35] } else { [145, 151, 155] };
                        match part {
                            Part::Point(i) => f.mark(cam.project(o.p[i]), c),
                            Part::Line(e) => {
                                f.line(cam.project(o.p[e[0]]), cam.project(o.p[e[1]]), c)
                            }
                            Part::Polygon(i) if on => {
                                for [x, y] in edges(&o.f[i]) {
                                    f.line(cam.project(o.p[x]), cam.project(o.p[y]), c)
                                }
                            }
                            _ => {}
                        }
                    }
                } else {
                    let q = cam.project(o.center);
                    for (dx, dy) in [(-2., 0.), (2., 0.), (0., -2.), (0., 2.)] {
                        f.mark([q[0] + dx, q[1] + dy, q[2]], [125, 225, 100])
                    }
                }
            }
        }
        for l in self.strokes.windows(2) {
            f.line(cam.project(l[0]), cam.project(l[1]), [255, 205, 45])
        }
        for &p in &self.strokes {
            f.mark(cam.project(p), [255, 205, 45])
        }
        let path = self.region_path();
        for l in path.windows(2) {
            f.line(
                [l[0][0], l[0][1], 1.],
                [l[1][0], l[1][1], 1.],
                [255, 190, 45],
            )
        }
    }
    fn region_path(&self) -> Vec<[f64; 2]> {
        if self.region == 1 && self.path.len() >= 2 {
            let a = self.path[0];
            let b = self.path[self.path.len() - 1];
            vec![a, [b[0], a[1]], b, [a[0], b[1]], a]
        } else {
            self.path.clone()
        }
    }
    fn enter_edit(&mut self, a: &mut App, f: &Frame) -> Result<(), String> {
        if a.edit {
            self.cancel(a);
            a.edit = false;
            a.face = None;
            a.sel.parts.clear();
            if let Some(v) = self.saved_view.take() {
                a.view = v
            }
            return Ok(());
        }
        self.saved_view = Some(a.view.clone());
        let [x, y] = self.pos(f);
        let obj = f.hit(x, y).map(|id| id.0).or(a.target);
        let i = match obj {
            Some(i) => i,
            None => {
                a.command("/add box")?;
                a.active()?
            }
        };
        let o = &a.scene.objects[i];
        if o.star.is_some() {
            return Err("choose a mesh to edit".into());
        }
        a.flight.stop();
        a.target = Some(i);
        a.face = None;
        a.edit = true;
        a.undo_floor = a.undo.len();
        a.sel = Selection {
            mode: 1,
            ..Selection::default()
        };
        let eye = a.view.eye();
        if len(sub(eye, o.center)) > o.radius {
            a.view.center = o.center;
            a.view.dist = 3.5 * o.radius.max(0.1);
            a.view.radius = o.radius.max(0.1);
            a.view.zoom = 1.;
        }
        Ok(())
    }
    fn paint(a: &mut App, fi: usize) -> Result<(), String> {
        let i = a.active()?;
        let s = a.surface();
        a.snapshot();
        let o = &mut a.scene.objects[i];
        let f = o.f.get_mut(fi).ok_or("no face")?;
        f.mat = s.mat;
        f.emit = s.emit;
        f.color = s.color;
        o.dirty = true;
        Ok(())
    }
    fn mouse(
        &mut self,
        a: &mut App,
        f: &Frame,
        code: u16,
        x: usize,
        row: usize,
        up: bool,
    ) -> Result<(), String> {
        let pos = [x.min(f.w - 1), (row * 2).min(f.h - 1)];
        self.pointer = Some(pos);
        let [x, y] = pos;
        if code & 64 != 0 {
            if let Some(t) = &mut self.tool {
                if t.op == 'b' {
                    t.segments = (t.segments as isize + if code & 1 == 0 { 1 } else { -1 })
                        .clamp(1, 32) as usize;
                    return self.preview(a, f, false);
                }
            }
            if a.view.dist == 0. && !a.view.sky {
                let dir = a.view.basis()[0];
                a.view.center = add(
                    a.view.center,
                    mul(dir, if code & 1 == 0 { 0.2 } else { -0.2 }),
                )
            } else {
                a.view.zoom =
                    (a.view.zoom * if code & 1 == 0 { 1.12 } else { 1. / 1.12 }).clamp(0.01, 1e6)
            }
            return Ok(());
        }
        if self.tool.is_some() {
            if up {
                return Ok(());
            }
            self.tool.as_mut().unwrap().pos = pos;
            return self.preview(a, f, code & 32 == 0);
        }
        if code & 32 != 0 {
            if let Some(old) = self.drag {
                if old != pos {
                    self.moved = true;
                    if self.region != 0 {
                        self.path.push([x as f64, y as f64])
                    } else {
                        a.view.yaw += (x as f64 - old[0] as f64) * 4.;
                        a.view.pitch =
                            (a.view.pitch + (y as f64 - old[1] as f64) * 4.).clamp(-89.9, 89.9)
                    }
                    self.drag = Some(pos)
                }
            }
            return Ok(());
        }
        if code & 3 != 0 {
            return Ok(());
        }
        if !up {
            self.drag = Some(pos);
            self.moved = false;
            if self.region != 0 {
                self.path = vec![[x as f64, y as f64]]
            }
            return Ok(());
        }
        if self.drag.take().is_none() {
            return Ok(());
        }
        if self.region != 0 {
            self.path.push([x as f64, y as f64]);
            edit::region(a, &self.region_path(), code & 4 != 0);
            self.region = 0;
            self.path.clear();
            return Ok(());
        }
        if self.moved {
            return Ok(());
        }
        if let Some((oi, fi)) = f.hit(x, y) {
            if code & 16 != 0 {
                a.target = Some(oi);
                return Self::paint(a, fi);
            }
        }
        if a.edit {
            let part = edit::pick(a, f, x, y);
            if code & 4 == 0 {
                a.sel.parts.clear()
            }
            if let Some(p) = part {
                if !a.sel.parts.insert(p) {
                    a.sel.parts.remove(&p);
                }
                a.face = if let Part::Polygon(i) = p {
                    Some(i)
                } else {
                    None
                }
            } else {
                a.face = None
            }
        } else if let Some((oi, _)) = f.hit(x, y) {
            a.aim(oi);
            a.tab.clear()
        }
        Ok(())
    }
    fn preview(&mut self, a: &mut App, f: &Frame, commit: bool) -> Result<(), String> {
        let t = self.tool.as_ref().ok_or("no tool")?;
        let i = a.active()?;
        let cam = a.view.camera(f.w, f.h);
        let (o, sel) = t.apply(&cam, &a.surface())?;
        if commit {
            let original = self.tool.take().unwrap().original;
            a.scene.objects[i] = original;
            a.snapshot();
        }
        a.scene.objects[i] = o;
        a.sel = sel;
        a.face = a.sel.faces().first().copied();
        Ok(())
    }
    fn tool_key(&mut self, a: &mut App, f: &Frame, k: Key) -> Result<(), String> {
        let t = self.tool.as_mut().unwrap();
        match k {
            Key::Esc => {
                self.cancel(a);
                return Ok(());
            }
            Key::Enter => return self.preview(a, f, true),
            Key::Char(c @ ('x' | 'y' | 'z' | 'X' | 'Y' | 'Z')) => {
                t.axis = Some((c.to_ascii_lowercase() as u8 - b'x') as usize)
            }
            Key::Char('-') if t.op == 'm' && t.axis.is_none() => t.minus = true,
            Key::Char(c) if c.is_ascii_digit() || matches!(c, '.' | '-' | '+') => t.number.push(c),
            Key::Back => {
                t.number.pop();
            }
            Key::Arrow('A' | 'B') if t.op == 'b' => {
                t.segments = (t.segments as isize + if k == Key::Arrow('A') { 1 } else { -1 })
                    .clamp(1, 32) as usize
            }
            _ => return Ok(()),
        }
        if matches!(t.number.as_str(), "-" | "." | "+") {
            return Ok(());
        }
        self.preview(a, f, false)
    }
    fn finish_draw(&mut self, a: &mut App) -> Result<(), String> {
        let i = a.active()?;
        let surface = a.surface();
        let pts = &self.strokes;
        let o = &a.scene.objects[i];
        match self.draw {
            '5' => {
                if pts.len() < 2 {
                    return Err("place at least two stroke points with V".into());
                }
                let closed = pts.len() > 2
                    && len(sub(pts[0], *pts.last().unwrap())) <= len(sub(pts[1], pts[0])) / 3.;
                let mut o = o.clone();
                let start = o.p.len();
                let ids: Vec<_> = (start..start + pts.len() - usize::from(closed)).collect();
                o.p.extend_from_slice(&pts[..ids.len()]);
                if closed && ids.len() >= 3 {
                    o.f.push(Face { v: ids, ..surface });
                    a.sel.parts = [Part::Polygon(o.f.len() - 1)].into();
                    a.sel.mode = 3
                } else {
                    for e in ids.windows(2) {
                        o.wires.push([e[0], e[1]])
                    }
                    a.sel.mode = 2;
                    a.sel.parts = o
                        .wires
                        .iter()
                        .filter(|e| e[0] >= start)
                        .map(|&e| Part::Line(edge(e[0], e[1])))
                        .collect()
                }
                o.refresh();
                a.snapshot();
                a.scene.objects[i] = o
            }
            '4' => {
                if !(2..=3).contains(&pts.len()) {
                    return Err("V places two or three points".into());
                }
                let fi = a
                    .face
                    .or_else(|| a.sel.faces().first().copied())
                    .ok_or("select the cutting face")?;
                let n = if pts.len() == 3 {
                    cross(sub(pts[1], pts[0]), sub(pts[2], pts[0]))
                } else {
                    cross(sub(pts[1], pts[0]), o.f[fi].normal)
                };
                let name = o.name.clone();
                let halves = saw(o, pts[0], n, &surface)?;
                let mut pieces = halves.into_iter().flat_map(parts);
                let front = pieces.next().ok_or("empty cut")?;
                a.snapshot();
                a.scene.objects[i] = front;
                for (k, mut part) in pieces.enumerate() {
                    part.name = format!("{name} cut {}", k + 1);
                    let raw = std::sync::Arc::make_mut(&mut part.original);
                    raw.take("id");
                    a.scene.objects.push(part)
                }
                a.sel.parts.clear();
                a.face = None
            }
            '6' => {
                let Some(count) = &self.lathe_count else {
                    if pts.len() < 2 {
                        return Err("V places the lathe contour".into());
                    }
                    self.lathe_count = Some(String::new());
                    return Ok(());
                };
                let n = if count.is_empty() {
                    7
                } else {
                    count.parse().map_err(|_| "segment count")?
                };
                if n < 4 {
                    return Err("at least 4 segments".into());
                }
                let fi = a
                    .face
                    .or_else(|| a.sel.faces().first().copied())
                    .ok_or("select a face to set the lathe axis")?;
                let face = &o.f[fi];
                let [a0, b0] = edges(face)
                    .min_by(|a, b| {
                        let h = |e: &[usize; 2]| o.p[e[0]][1] + o.p[e[1]][1];
                        h(a).total_cmp(&h(b))
                    })
                    .ok_or("face has no edge")?;
                let origin = o.p[a0];
                let axis = norm(sub(o.p[b0], origin));
                let mut up = norm(cross(face.normal, axis));
                if dot(sub(face.mid, origin), up) < 0. {
                    up = mul(up, -1.)
                }
                let profile: Vec<_> = pts
                    .iter()
                    .map(|p| {
                        let q = sub(*p, origin);
                        [dot(q, axis), dot(q, up).max(0.)]
                    })
                    .collect();
                let lathe = lathe("Lathe", &profile, origin, axis, up, n, &surface)?;
                a.snapshot();
                a.scene.objects.push(lathe);
                a.target = Some(a.scene.objects.len() - 1);
                a.sel.parts.clear();
                a.face = None
            }
            _ => {}
        }
        self.draw = '\0';
        self.strokes.clear();
        self.lathe_count = None;
        Ok(())
    }
    fn key(&mut self, a: &mut App, f: &Frame, k: Key) -> Result<(), String> {
        let k = match k {
            Key::Roll(c, down) => {
                let idx = usize::from(matches!(c, ']' | 'ú'));
                if !down || matches!(self.panel, Panel::Scene) && a.scene.sky && self.tool.is_none()
                {
                    a.flight.roll_keys[idx] = down && a.scene.sky;
                    return Ok(());
                }
                Key::Char(c)
            }
            Key::Repeat(_) if matches!(self.panel, Panel::Scene) && self.tool.is_none() => {
                return Ok(())
            }
            Key::Repeat(c) => Key::Char(c),
            Key::Ignored => return Ok(()),
            other => other,
        };
        if k == Key::Ctrl(3) {
            self.quit = true;
            return Ok(());
        }
        match self.panel {
            Panel::Find(_) => return self.find_key(a, k),
            Panel::Palette => {
                let left = f.w.saturating_sub((f.w * 45 / 100).clamp(24, 36));
                if let Key::Mouse { code, x, y, up } = k {
                    if x < left {
                        return self.mouse(a, f, code, x, y, up);
                    }
                    return self.palette_key(
                        a,
                        Key::Mouse {
                            code,
                            x: x - left,
                            y,
                            up,
                        },
                        f.h / 2,
                    );
                }
                return self.palette_key(a, k, f.h / 2);
            }
            Panel::Help(i) => {
                let h = f.h / 2;
                let count = md::document(md::PAGES[i].1, "md", f.w).len();
                match k {
                    Key::Esc | Key::Char('?') => self.panel = Panel::Scene,
                    Key::Arrow('C' | 'D') | Key::Tab(_) => {
                        let prev = matches!(k, Key::Arrow('D') | Key::Tab(true));
                        self.panel = Panel::Help(
                            (i + if prev { md::PAGES.len() - 1 } else { 1 }) % md::PAGES.len(),
                        );
                        self.scroll = 0
                    }
                    Key::Arrow('A') => self.scroll = self.scroll.saturating_sub(1),
                    Key::Arrow('B') => self.scroll += 1,
                    Key::Page(down) => {
                        self.scroll = if down {
                            self.scroll + h - 2
                        } else {
                            self.scroll.saturating_sub(h - 2)
                        }
                    }
                    Key::Home(end) => self.scroll = if end { count } else { 0 },
                    Key::Enter | Key::Char('/') => {
                        self.panel = Panel::Find("/help ".into());
                        self.index = 0
                    }
                    Key::Mouse { code, .. } if code & 64 != 0 => {
                        self.scroll = if code & 1 == 0 {
                            self.scroll.saturating_sub(3)
                        } else {
                            self.scroll + 3
                        }
                    }
                    _ => {}
                }
                self.scroll = self.scroll.min(count.saturating_sub(1));
                return Ok(());
            }
            _ => {}
        }
        if let Key::Mouse { code, x, y, up } = k {
            return self.mouse(a, f, code, x, y, up);
        }
        if self.tool.is_some() {
            return self.tool_key(a, f, k);
        }
        if self.draw != '\0' {
            match k {
                Key::Char('v' | 'V') => {
                    let [x, y] = self.pos(f);
                    if self.draw == '6' {
                        let o = &a.scene.objects[a.active()?];
                        let fi = a
                            .face
                            .or_else(|| a.sel.faces().first().copied())
                            .ok_or("select a lathe face")?;
                        let f0 = &o.f[fi];
                        let cam = a.view.camera(f.w, f.h);
                        let p = cam.point(x as f64 + 0.5, y as f64 + 0.5, 0.);
                        let ray = sub(cam.point(x as f64 + 0.5, y as f64 + 0.5, 1.), p);
                        let den = dot(ray, f0.normal);
                        if den.abs() > 1e-9 {
                            self.strokes
                                .push(add(p, mul(ray, dot(sub(f0.mid, p), f0.normal) / den)))
                        }
                        return Ok(());
                    }
                    if let Some((oi, fi)) = f.hit(x, y) {
                        if Some(oi) == a.target {
                            if self.strokes.is_empty() && self.draw == '4' {
                                a.face = Some(fi)
                            }
                            self.strokes.push(edit::world(a, f, x, y))
                        }
                    }
                }
                Key::Enter | Key::Char('5') => self.finish_draw(a)?,
                Key::Char(c) if self.lathe_count.is_some() && c.is_ascii_digit() => {
                    self.lathe_count.as_mut().unwrap().push(c)
                }
                Key::Back => {
                    if let Some(s) = &mut self.lathe_count {
                        s.pop();
                    } else {
                        self.strokes.pop();
                    }
                }
                Key::Esc => {
                    if self.draw == '6' && self.strokes.pop().is_some() {
                    } else {
                        self.cancel(a)
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if a.edit {
            let op = match k {
                Key::Char(c @ ('g' | 'e' | 'r' | 's' | 'm')) => Some(c),
                Key::Ctrl(2) => Some('b'),
                Key::Ctrl(18) => Some('l'),
                Key::Ctrl(5) => Some('e'),
                _ => None,
            };
            if let Some(op) = op {
                self.tool = Some(Transform::new(a, op, self.pos(f), k == Key::Ctrl(5))?);
                return Ok(());
            }
            match k {
                Key::Char(c @ ('1' | '2' | '3')) => {
                    a.sel.mode = c as u8 - b'0';
                    a.sel.parts.clear();
                    a.face = None;
                    return Ok(());
                }
                Key::Char('a') => {
                    let i = a.active()?;
                    if self
                        .all
                        .is_some_and(|t| t.elapsed() < Duration::from_millis(400))
                    {
                        a.sel.parts.clear();
                        self.all = None
                    } else {
                        a.sel.parts = a.sel.candidates(&a.scene.objects[i]).into_iter().collect();
                        self.all = Some(Instant::now())
                    }
                    return Ok(());
                }
                Key::Char('l') => {
                    let i = a.active()?;
                    let o = &a.scene.objects[i];
                    let p = a
                        .sel
                        .parts
                        .iter()
                        .next()
                        .copied()
                        .ok_or("select an edge or face")?;
                    let e = match p {
                        Part::Line(e) => e,
                        Part::Polygon(fi) => {
                            let f = &o.f[fi];
                            if f.v.len() != 4 {
                                return Err("loop needs quad faces".into());
                            }
                            let j = usize::from(self.alternate_ring);
                            self.alternate_ring = !self.alternate_ring;
                            edge(f.v[j], f.v[j + 1])
                        }
                        _ => return Err("select an edge or face".into()),
                    };
                    let ring = quad_ring(o, e);
                    a.sel.parts.clear();
                    for (fi, j) in ring {
                        if a.sel.mode == 3 {
                            a.sel.parts.insert(Part::Polygon(fi));
                        } else {
                            let f = &o.f[fi];
                            for k in [j, (j + 2) % 4] {
                                a.sel
                                    .parts
                                    .insert(Part::Line(edge(f.v[k], f.v[(k + 1) % 4])));
                            }
                        }
                    }
                    return Ok(());
                }
                Key::Char('b') => {
                    self.region = if self.region == 1 { 2 } else { 1 };
                    return Ok(());
                }
                Key::Char(c @ ('4' | '5' | '6')) => {
                    self.draw = c;
                    return Ok(());
                }
                Key::Alt('n' | 'N') => {
                    let i = a.active()?;
                    let ids = a.sel.faces();
                    a.snapshot();
                    let o = &mut a.scene.objects[i];
                    for (j, f) in o.f.iter_mut().enumerate() {
                        if ids.is_empty() || ids.contains(&j) {
                            f.v.reverse()
                        }
                    }
                    o.refresh();
                    return Ok(());
                }
                Key::Char('f') => {
                    let i = a.active()?;
                    let mut o = a.scene.objects[i].clone();
                    fill(&mut o, &a.sel, &a.surface())?;
                    a.snapshot();
                    a.scene.objects[i] = o;
                    return Ok(());
                }
                Key::Esc if self.region == 0 => return self.enter_edit(a, f),
                Key::Esc if self.region != 0 => {
                    self.cancel(a);
                    return Ok(());
                }
                _ => {}
            }
        }
        match k {
            Key::Enter | Key::ShiftEnter => {
                self.panel = Panel::Find(String::new());
                self.index = 0
            }
            Key::Char('?' | 'h') => self.help("", f.w),
            Key::Char('p' | 'P') => self.panel = Panel::Palette,
            Key::Char('q' | 'Q') => self.enter_edit(a, f)?,
            Key::Char('b' | 'B') if !a.edit => {
                let [x, y] = self.pos(f);
                if let Some(i) = f.hit(x, y).map(|v| v.0).or(a.target) {
                    if a.operands.len() == 2 {
                        a.operands.clear()
                    }
                    if !a.operands.contains(&i) {
                        a.operands.push(i)
                    }
                    a.note = a
                        .operands
                        .iter()
                        .map(|&i| a.scene.objects[i].name.as_str())
                        .collect::<Vec<_>>()
                        .join(" + ")
                }
            }
            Key::Char(c @ ('1' | '2' | '3' | '4')) if !a.edit && a.operands.len() == 2 => {
                a.command(
                    [
                        "/bool union",
                        "/bool subtract",
                        "/bool subtract-swap",
                        "/bool intersect",
                    ][(c as u8 - b'1') as usize],
                )?;
            }
            Key::Char('t' | 'T') => {
                let [x, y] = self.pos(f);
                if let Some(i) = f
                    .hit(x, y)
                    .map(|p| p.0)
                    .or(a.target)
                    .or_else(|| a.target_order().first().copied())
                {
                    a.aim(i);
                    a.tab.clear()
                }
            }
            Key::Tab(rev) => a.cycle(rev),
            Key::Esc => {
                if a.flight.orbit {
                    a.view.free();
                }
                a.target = None;
                a.face = None;
                a.tab.clear();
                a.flight.orbit = false;
                a.flight.arming = None;
                a.flight.steer = false;
            }
            Key::Char(' ') => {
                a.flight.stop();
            }
            Key::Char(';' | 'é') => a.flight.frozen = !a.flight.frozen,
            Key::Char('z' | 'Z') => self.zen = !self.zen,
            Key::Char('u' | 'U') => {
                a.command("/undo")?;
            }
            Key::Char('[' | 'ő' | ']' | 'ú') if a.scene.sky => {
                a.view.roll += if matches!(k, Key::Char('[' | 'ő')) {
                    -5.
                } else {
                    5.
                }
            }
            Key::Char('0' | 'ö') => {
                if a.scene.sky && !a.flight.frozen {
                    if let Some(i) = a.target {
                        let o = &a.scene.objects[i];
                        if a.flight.orbit {
                            let radial = sub(a.view.eye(), o.center);
                            let b = a.view.basis();
                            let tang = add(
                                mul(b[1], Flight::pace(a.flight.levels[1])),
                                mul(b[2], Flight::pace(a.flight.levels[2])),
                            );
                            a.flight.stop();
                            a.flight.cruise = mul(tang, len(radial) * 0.175);
                            a.flight.gravity_at(&a.view, o.center);
                            a.view.free();
                        } else if a.flight.arming.is_some() {
                            a.flight.arming = None
                        } else {
                            a.flight.steer = true;
                            a.flight.arming = Some(-1.);
                            a.flight.gravity = None;
                        }
                    } else if a.flight.gravity.take().is_none() {
                        let cam = a.view.camera(f.w, f.h);
                        if let Some(o) = a
                            .scene
                            .objects
                            .iter()
                            .filter(|o| {
                                let p = cam.project(o.center);
                                p[2] > 0.
                                    && p[0] >= 0.
                                    && p[0] < cam.w
                                    && p[1] >= 0.
                                    && p[1] < cam.h
                            })
                            .min_by(|a, b| {
                                len(sub(cam.eye, a.center)).total_cmp(&len(sub(cam.eye, b.center)))
                            })
                        {
                            a.flight.gravity_at(&a.view, o.center)
                        }
                    }
                } else if !a.scene.sky {
                    a.reframe()
                }
            }
            Key::Arrow(c) => match c {
                'A' => a.view.pitch += 5.,
                'B' => a.view.pitch -= 5.,
                'C' => a.view.yaw += 5.,
                'D' => a.view.yaw -= 5.,
                _ => {}
            },
            Key::Char('R') if !a.edit => {
                self.panel = Panel::Find("/r ".into());
                self.index = 0
            }
            Key::Char(c) => {
                if let Some((axis, step)) = match c.to_ascii_lowercase() {
                    'w' => Some((0, 1)),
                    's' => Some((0, -1)),
                    'd' => Some((1, 1)),
                    'a' => Some((1, -1)),
                    'r' => Some((2, 1)),
                    'f' => Some((2, -1)),
                    _ => None,
                } {
                    a.flight.speed(axis, step)
                }
            }
            _ => {}
        }
        Ok(())
    }
}
pub fn run(mut a: App, mut session: Option<Session>) -> Result<(), String> {
    let _term = Terminal::open().map_err(|e| e.to_string())?;
    let mut ui = Ui::default();
    let mut stdout = io::stdout().lock();
    let mut stdin = io::stdin().lock();
    let mut input = Vec::new();
    let mut last_input = Instant::now();
    let mut incomplete = false;
    let mut last = Instant::now();
    let mut poll = last;
    let mut dirty = true;
    let mut dims = (0, 0, 0.);
    let mut old = vec![];
    let mut was_scene = false;
    let mut frame = Frame::new(1, 2);
    while !ui.quit && !Terminal::stopped() {
        let now = Instant::now();
        let dt = now.duration_since(last).as_secs_f64().min(0.05);
        last = now;
        if poll.elapsed() >= Duration::from_millis(150) {
            poll = now;
            if let Some(s) = session.as_mut() {
                match s.refresh() {
                    Ok(Some(scene)) => {
                        ui.cancel(&mut a);
                        a.reload(scene);
                        dirty = true
                    }
                    Err(e) if a.note != e => {
                        a.note = e;
                        dirty = true
                    }
                    _ => {}
                }
            }
        }
        let (w, rows, aspect) = terminal::size();
        let h = rows - 2;
        if dims != (w, h, aspect) {
            let first = dims.0 == 0;
            dims = (w, h, aspect);
            a.size = (w, h * 2);
            a.view.aspect = aspect;
            if first {
                a.reframe()
            }
            old.clear();
            was_scene = false;
            write!(stdout, "\x1b[2J").map_err(|e| e.to_string())?;
            dirty = true;
        }
        if matches!(ui.panel, Panel::Scene) && ui.tool.is_none() {
            dirty |= a.flight.tick(&mut a.view, &a.scene, a.target, dt)
        }
        if dirty {
            let mut output = String::new();
            let mut lines = if matches!(ui.panel, Panel::Palette) {
                let panel_w = (w * 45 / 100).clamp(24, 36).min(w);
                let left = w - panel_w;
                frame = render(&a.scene, &a.view, w, h * 2, a.target, a.face);
                ui.decorate(&a, &mut frame);
                let panel = ui.overlay(&a, panel_w, h).unwrap_or_default();
                if was_scene {
                    old.clear()
                }
                was_scene = false;
                (0..h)
                    .map(|y| {
                        let mut line = String::new();
                        frame.span(y * 2, 0, left, &mut line);
                        if let Some(content) = panel.get(y) {
                            line.push_str(content)
                        }
                        line
                    })
                    .collect()
            } else if let Some(mut panel) = ui.overlay(&a, w, h) {
                if was_scene {
                    old.clear()
                }
                was_scene = false;
                panel.truncate(h);
                panel.resize(h, String::new());
                panel
            } else {
                let mut next = render(&a.scene, &a.view, w, h * 2, a.target, a.face);
                ui.decorate(&a, &mut next);
                next.diff(was_scene.then_some(&frame), &mut output);
                frame = next;
                was_scene = true;
                vec![String::new(); h]
            };
            let target = a.target.and_then(|i| a.scene.objects.get(i));
            let distance = target.map(|o| (len(sub(a.view.eye(), o.center)) - o.radius).max(0.));
            let mode = if a.edit {
                "Q edit"
            } else if a.flight.orbit {
                "ORBIT"
            } else if a.flight.arming.is_some() {
                "orbit brake/arm"
            } else if a.flight.gravity.is_some() {
                "gravity"
            } else {
                "view"
            };
            let mut status = format!(
                "{BUILD} | {mode} | {} objects | {} {} | W{} D{} R{}",
                a.scene.objects.len(),
                target.map_or("-", |o| o.name.as_str()),
                distance.map_or(String::new(), |v| if a.scene.sky {
                    sky_distance(v)
                } else {
                    format!("{v:.4} m")
                }),
                a.flight.levels[0],
                a.flight.levels[1],
                a.flight.levels[2]
            );
            if a.scene.sky {
                let v = a.flight.indicated_speed(&a.view, target.is_some());
                status.push_str(&format!(
                    " | {}/s · {} c",
                    sky_distance(v),
                    sky_quantity(v * 31_557_600.)
                ));
            }
            lines.push(if ui.zen {
                String::new()
            } else {
                safe(&status, w)
            });
            let note = if let Some(t) = &ui.tool {
                format!(
                    "{} {} {} · Enter accepts, Esc cancels",
                    t.op,
                    t.axis.map_or("", |i| ["X", "Y", "Z"][i]),
                    t.number
                )
            } else if ui.lathe_count.is_some() {
                format!(
                    "Lathe segments [{}] Enter",
                    ui.lathe_count.as_deref().unwrap_or("")
                )
            } else if ui.draw != '\0' {
                format!(
                    "{} · V place point · Enter finish · {} points",
                    ui.draw,
                    ui.strokes.len()
                )
            } else if ui.region != 0 {
                format!(
                    "{} · drag to select",
                    if ui.region == 1 { "Box" } else { "Lasso" }
                )
            } else {
                a.note.clone()
            };
            lines.push(safe(&note, w));
            for (y, l) in lines.iter().enumerate() {
                if (!was_scene || y >= h) && old.get(y) != Some(l) {
                    use std::fmt::Write;
                    write!(output, "\x1b[{};1H{}\x1b[0m\x1b[K", y + 1, l).unwrap()
                }
            }
            if !output.is_empty() {
                write!(stdout, "\x1b[?2026h{output}\x1b[?2026l")
                    .and_then(|_| stdout.flush())
                    .map_err(|e| e.to_string())?
            }
            old = lines;
            dirty = false
        }
        let moving = matches!(ui.panel, Panel::Scene) && a.flight.active();
        let timeout = if !input.is_empty() {
            if incomplete {
                20
            } else {
                0
            }
        } else if moving {
            16
        } else {
            100
        };
        if terminal::Terminal::ready(timeout).map_err(|e| e.to_string())? {
            let mut buf = [0; 8192];
            let n = stdin.read(&mut buf).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            input.extend_from_slice(&buf[..n]);
            last_input = Instant::now()
        }
        incomplete = false;
        if let Some(k) = terminal::decode(
            &mut input,
            last_input.elapsed() >= Duration::from_millis(18),
        ) {
            if let Key::Mouse { code, x, y, .. } = k {
                if code & 32 != 0 && ui.drag.is_none() && ui.tool.is_none() {
                    ui.pointer = Some([x.min(w - 1), (y * 2).min(h * 2 - 1)]);
                    continue;
                }
            }
            let generation = a.generation;
            match ui.key(&mut a, &frame, k) {
                Ok(()) => {
                    if generation != a.generation {
                        if let Some(s) = session.as_mut() {
                            if let Err(e) = s.publish(&a.scene) {
                                ui.cancel(&mut a);
                                a.reload(s.last_valid());
                                a.note = e
                            }
                        }
                    }
                }
                Err(e) => a.note = e,
            }
            dirty = true;
        } else {
            incomplete = !input.is_empty();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_region_preview_cancel_paint_and_undo() {
        let mut a = App::new(Scene {
            objects: vec![Object::cube("Block".into(), [150, 150, 150])],
            ..Scene::default()
        });
        let mut ui = Ui::default();
        let frame = |a: &App| render(&a.scene, &a.view, 80, 48, a.target, a.face);
        let f = frame(&a);
        ui.key(&mut a, &f, Key::Char('q')).unwrap();
        let f = frame(&a);
        let original = a.scene.objects[0].p.clone();
        for (mode, count) in [('1', 8), ('2', 12), ('3', 6)] {
            ui.key(&mut a, &f, Key::Char(mode)).unwrap();
            ui.key(&mut a, &f, Key::Char('b')).unwrap();
            if mode == '2' {
                ui.key(&mut a, &f, Key::Char('b')).unwrap();
            }
            ui.mouse(&mut a, &f, 0, 0, 0, false).unwrap();
            for (x, y) in [(79, 0), (79, 23), (0, 23)] {
                ui.mouse(&mut a, &f, 32, x, y, false).unwrap();
            }
            let end = if mode == '2' { (0, 0) } else { (79, 23) };
            ui.mouse(&mut a, &f, 0, end.0, end.1, true).unwrap();
            assert_eq!(a.sel.parts.len(), count);
        }
        ui.key(&mut a, &f, Key::Char('g')).unwrap();
        ui.mouse(&mut a, &f, 35, 60, 17, false).unwrap();
        assert_ne!(a.scene.objects[0].p, original);
        ui.key(&mut a, &f, Key::Esc).unwrap();
        assert_eq!(a.scene.objects[0].p, original);
        let (oi, fi) = f.hit(40, 24).unwrap();
        a.material = "wood".into();
        ui.mouse(&mut a, &f, 16, 40, 12, false).unwrap();
        ui.mouse(&mut a, &f, 16, 40, 12, true).unwrap();
        assert_eq!(
            Scene::read(&a.scene.write()).unwrap().objects[oi].f[fi].mat,
            "wood"
        );
        ui.key(&mut a, &f, Key::Char('u')).unwrap();
        assert_ne!(a.scene.objects[oi].f[fi].mat, "wood");
        ui.key(&mut a, &f, Key::Esc).unwrap();
        assert!(!a.edit);
    }

    #[test]
    fn space_flight_units() {
        assert_eq!(sky_distance(0.), "0.00 ly");
        assert_eq!(sky_distance(1. / 63241.077), "1.0 AU");
        assert_eq!(sky_distance(300. / 9.4607304725808e12), "300 km");
        assert_eq!(sky_distance(2e6), "2.0M ly");
        let v = View {
            dist: 100.,
            ..View::default()
        };
        let mut f = Flight::default();
        f.speed(1, 1);
        assert!((f.indicated_speed(&v, true) - 17.5).abs() < 1e-9);
    }
}
