use crate::*;
use std::collections::BTreeMap;
fn cube() -> Object {
    Object::cube("Cube".into(), [100, 150, 200])
}
fn solid(o: &Object) {
    let mut uses = BTreeMap::new();
    for f in &o.f {
        assert!(f.v.len() >= 3);
        for e in edges(f) {
            assert!(e[0] < e[1] && e[1] < o.p.len());
            *uses.entry(e).or_insert(0) += 1
        }
    }
    assert!(uses.values().all(|n| *n == 2), "{uses:?}")
}
fn selection(part: Part) -> Selection {
    Selection {
        mode: match part {
            Part::Point(_) => 1,
            Part::Line(_) => 2,
            Part::Polygon(_) => 3,
        },
        parts: [part].into(),
    }
}
#[test]
fn all_models_round_trip() {
    let mut n = 0;
    for entry in fs::read_dir("models").unwrap() {
        let p = entry.unwrap().path();
        if p.extension().is_none_or(|v| v != "json") {
            continue;
        }
        let text = fs::read_to_string(&p).unwrap();
        let s = Scene::read(&text).unwrap();
        assert_eq!(
            json::parse(&s.write()).unwrap(),
            json::parse(&text).unwrap(),
            "{}",
            p.display()
        );
        n += 1
    }
    assert!(n >= 27)
}
#[test]
fn prototype_edit_is_local() {
    let original = load("lark-4").unwrap();
    let mut s = original.clone();
    let i = s
        .objects
        .iter()
        .position(|o| !o.original.get("proto").is_null())
        .unwrap();
    let o = &mut s.objects[i];
    let old = o.center;
    o.transform("g", 0, 2., &(0..o.p.len()).collect::<Vec<_>>());
    let doc = s.document();
    assert!(doc.get("objects").arr()[i].get("proto").is_null());
    assert_eq!(doc.get("prototypes"), original.root.get("prototypes"));
    let opened = Scene::read(&doc.encode()).unwrap();
    assert!((opened.objects[i].center[0] - old[0] - 2.).abs() < 1e-8);
    for j in 0..s.objects.len() {
        if i != j {
            assert_eq!(opened.objects[j].original, original.objects[j].original)
        }
    }
}
#[test]
fn extension_fields_survive_edits() {
    let src = r##"{"format":"3draft-scene","version":2,"story":{"active":false,"id":9007199254740993},"objects":[{"name":"Ω","custom":true,"wire":[[0,2]],"mesh":{"extra":"keep","positions":[[0,0,0],[1,0,0],[0,1,0]],"faces":[{"v":[0,1,2],"tag":"a","color":"#ff1111"}]}}]}"##;
    let mut s = Scene::read(src).unwrap();
    s.objects[0].transform("g", 1, 3., &[0, 1, 2]);
    let d = s.document();
    assert_eq!(d.get("story"), json::parse(src).unwrap().get("story"));
    let o = &d.get("objects").arr()[0];
    assert_eq!(o.get("mesh").get("extra").str(), "keep");
    assert_eq!(o.get("mesh").get("faces").arr()[0].get("tag").str(), "a");
    assert_eq!(o.get("wire").arr().len(), 1);
    assert_eq!(o.get("custom"), &J::Bool(true))
}
#[test]
fn strict_json_and_unicode() {
    for bad in ["01", "1.", "1e", "1e-", "[1,]", r#""\uD800""#] {
        assert!(json::parse(bad).is_err(), "{bad}")
    }
    let s = r#"{"n":9007199254740993,"x":"\ud83d\ude80","t":true,"f":false}"#;
    let j = json::parse(s).unwrap();
    assert!(j.encode().contains("9007199254740993"));
    assert_eq!(j.get("x").str(), "🚀");
    assert_eq!(json::parse(&j.encode()).unwrap(), j)
}
#[test]
fn model_orbit_is_time_based() {
    let s = Scene {
        objects: vec![cube()],
        ..Scene::default()
    };
    let mut a = App::new(s.clone());
    a.aim(0);
    let start = a.view.clone();
    for n in [30, 60, 120] {
        a.view = start.clone();
        a.flight.stop();
        a.flight.speed(1, 1);
        for _ in 0..n {
            assert!(a.flight.tick(&mut a.view, &s, Some(0), 1. / n as f64))
        }
        assert!((a.view.yaw - start.yaw + 0.175f64.to_degrees()).abs() < 1e-9);
        assert!((a.view.dist - start.dist).abs() < 1e-10)
    }
}
#[test]
fn free_flight_braking_orbit_arm() {
    let s = load("deepsky").unwrap();
    let mut v = View::home(&s, 100, 80, 1.);
    let mut f = Flight::default();
    let o = &s.objects[0];
    v.aim(o.center);
    v.free();
    let before = len(sub(v.eye(), o.center));
    f.speed(0, 18);
    for _ in 0..1800 {
        f.tick(&mut v, &s, Some(0), 1. / 60.);
        assert!(len(sub(v.eye(), o.center)) >= o.radius)
    }
    assert!(len(sub(v.eye(), o.center)) < before / 100.);
    f.arming = Some(-1.);
    for _ in 0..120 {
        f.tick(&mut v, &s, Some(0), 1. / 60.);
    }
    assert!(f.orbit);
    f.stop();
    assert!(!f.active());
    f.speed(1, 1);
    let eye = v.eye();
    f.tick(&mut v, &s, None, 0.1);
    assert_ne!(v.eye(), eye)
}
#[test]
fn projection_aspect_and_inverse() {
    let mut v = View::default();
    for aspect in [0.8, 1., 1.25] {
        v.aspect = aspect;
        for ortho in [false, true] {
            v.ortho = ortho;
            let c = v.camera(120, 80);
            let p = [0.3, 0.8, -0.2];
            let q = c.project(p);
            assert!(len(sub(c.point(q[0], q[1], q[2]), p)) < 1e-10);
            let a = c.project(add(p, c.r));
            let b = c.project(add(p, c.u));
            assert!(((a[0] - q[0]) - (q[1] - b[1]) * aspect).abs() < 1e-8)
        }
    }
}
#[test]
fn reference_globe_and_sky() {
    for (name, count) in [("globe", 3969), ("deepsky", 305)] {
        let s = load(name).unwrap();
        let v = View::home(&s, 120, 80, 1.);
        let f = render(&s, &v, 120, 80, None, None);
        let hit: Vec<_> = f.rgb.iter().filter(|c| **c != [12, 14, 18]).collect();
        assert_eq!(hit.len(), count);
        if name == "globe" {
            let mean = hit
                .iter()
                .map(|p| p.iter().map(|&v| v as f64).sum::<f64>())
                .sum::<f64>()
                / (3. * count as f64);
            assert!((mean - 88.4754).abs() < 0.2)
        }
        assert!(f.rows().join("").contains("38;2;"));
        let mut out = String::new();
        f.diff(Some(&f), &mut out);
        assert!(out.is_empty())
    }
}
#[test]
fn z_buffer_pick_and_diff() {
    let mut far = cube();
    far.transform("g", 2, -3., &(0..8).collect::<Vec<_>>());
    let s = Scene {
        objects: vec![cube(), far],
        ..Scene::default()
    };
    let v = View {
        yaw: 0.,
        pitch: 0.,
        center: [0., 0.5, 0.],
        ortho: true,
        ..View::default()
    };
    let f = render(&s, &v, 64, 64, None, None);
    assert_eq!(f.hit(32, 32).unwrap().0, 0);
    let mut g = render(&s, &v, 64, 64, None, None);
    g.rgb[0] = [0, 1, 2];
    let mut d = String::new();
    g.diff(Some(&f), &mut d);
    assert!(d.starts_with("\x1b[1;1H"));
    assert_eq!(d.matches('▀').count(), 1)
}
#[test]
fn primitives_closed() {
    for k in ["box", "cone", "prism", "pyramid", "sphere", "cylinder"] {
        let o = primitive(k, k, &Face::default()).unwrap();
        solid(&o);
        assert!(o.f.iter().all(|f| len(f.normal) > 0.99));
    }
}
#[test]
fn extrude_point_edge_region() {
    let mut o = cube();
    let s = selection(Part::Point(0));
    let p = o.p[0];
    extrude(&mut o, &s, 2., Some(2), false, &Face::default()).unwrap();
    assert_eq!(o.p[8], add(p, [0., 0., 2.]));
    assert_eq!(o.wires.len(), 1);
    let sel = selection(Part::Line([0, 1]));
    let f = o.f.len();
    extrude(&mut o, &sel, 1., Some(1), false, &Face::default()).unwrap();
    assert_eq!(o.f.len(), f + 1);
    let mut o = cube();
    let mut sel = selection(Part::Polygon(0));
    sel.parts.insert(Part::Polygon(2));
    extrude(&mut o, &sel, 0.2, None, false, &Face::default()).unwrap();
    solid(&o);
    assert_eq!(o.f.len(), 12);
}
#[test]
fn cut_bevel_mirror_fill() {
    let base = cube();
    let n = [0., 1., 0.];
    let [a, b] = saw(&base, [0., 0.5, 0.], n, &Face::default()).unwrap();
    solid(&a);
    solid(&b);
    assert!(a.p.iter().all(|p| p[1] >= 0.5));
    let mut o = base.clone();
    bevel(&mut o, &selection(Part::Line([0, 1])), 0.1, 3).unwrap();
    solid(&o);
    assert!(o.f.len() > base.f.len());
    let mut m = base.clone();
    mirror(&mut m, &Selection::default(), 0, 1., 0.).unwrap();
    solid(&m);
    assert!(m.p.iter().any(|p| p[0] == 1.5));
    let mut o = base;
    let cap = o.f.pop().unwrap();
    let sel = Selection {
        mode: 1,
        parts: cap.v.iter().copied().map(Part::Point).collect(),
    };
    fill(&mut o, &sel, &cap).unwrap();
    solid(&o)
}
#[test]
fn loop_cut_and_lathe() {
    let mut o = cube();
    let selection = loop_cut(&mut o, [0, 1], 2, 0.).unwrap();
    solid(&o);
    assert_eq!(selection.parts.len(), 8);
    let cup = lathe(
        "cup",
        &[[0., 0.38], [1., 0.5], [1., 0.44], [0.1, 0.38], [0.1, 0.]],
        [0.; 3],
        [0., 1., 0.],
        [1., 0., 0.],
        24,
        &Face::default(),
    )
    .unwrap();
    solid(&cup)
}
#[test]
fn keyboard_and_markdown() {
    use terminal::{decode, Key};
    let mut seq = b"\x1b".to_vec();
    assert!(decode(&mut seq, false).is_none());
    seq.extend_from_slice(b"[Z");
    assert_eq!(decode(&mut seq, false), Some(Key::Tab(true)));
    let mut s = "ő\x1bn\x1b[<32;14;9M".as_bytes().to_vec();
    assert_eq!(decode(&mut s, false), Some(Key::Char('ő')));
    assert_eq!(decode(&mut s, false), Some(Key::Alt('n')));
    assert_eq!(
        decode(&mut s, false),
        Some(Key::Mouse {
            code: 32,
            x: 13,
            y: 8,
            up: false
        })
    );
    for (name, s) in md::PAGES {
        let lines = md::document(s, "md", 40);
        assert!(
            lines
                .iter()
                .all(|l| md::plain(l).chars().map(md::cells).sum::<usize>() <= 40),
            "{name}"
        );
    }
    assert!(md::document("fn main() {}", "a.rs", 80)[0].contains("172;214;65mfn"));
    assert!(
        md::document("# Árvíz\n- **bold** [doc](edit.md)", "a.md", 40)
            .join("")
            .contains("edit.md")
    )
}
#[test]
fn target_cycle_and_reload() {
    let mut a = App::new(load("deepsky").unwrap());
    assert_eq!(a.find("Sun")[0], 0);
    a.cycle(false);
    let i = a.target.unwrap();
    a.cycle(false);
    a.cycle(true);
    assert_eq!(a.target, Some(i));
    let eye = a.view.eye();
    let mut s = a.scene.clone();
    s.objects.reverse();
    a.reload(s);
    assert_eq!(a.target, Some(5148 - i));
    assert_eq!(a.view.eye(), eye);
}
#[test]
fn boolean_and_multi_edge_bevel() {
    let a = cube();
    let mut b = cube();
    b.transform("g", 0, 0.5, &(0..8).collect::<Vec<_>>());
    for (op, volume) in [("union", 1.5), ("intersect", 0.5), ("subtract", 0.5)] {
        let out = csg::boolean(&a, &b, op).unwrap();
        solid(&out);
        let mut sum = 0.;
        for f in &out.f {
            for i in 1..f.v.len() - 1 {
                sum += dot(out.p[f.v[0]], cross(out.p[f.v[i]], out.p[f.v[i + 1]])) / 6.;
            }
        }
        assert!((sum - volume).abs() < 1e-8)
    }
    let mut o = cube();
    let s = Selection {
        mode: 2,
        parts: [Part::Line([0, 1]), Part::Line([0, 3]), Part::Line([0, 4])].into(),
    };
    bevel(&mut o, &s, 0.08, 4).unwrap();
    solid(&o)
}

#[test]
fn physical_keys_roll_and_release() {
    use terminal::{decode, Key};
    for (bytes, key) in [
        ("\x1b[100;1:2;100u", Key::Repeat('d')),
        ("\x1b[100;1:3u", Key::Ignored),
        ("\x1b[93;1u", Key::Roll(']', true)),
        ("\x1b[93;1:3u", Key::Roll(']', false)),
        ("\x1b[114;5u", Key::Ctrl(18)),
        ("\x1b[110;3u", Key::Alt('n')),
        ("\x1b[13;2u", Key::ShiftEnter),
        ("\x1b\r", Key::ShiftEnter),
        ("\x1b[97:65;2;65u", Key::Char('A')),
        ("\x1b[1;1:3A", Key::Ignored),
    ] {
        assert_eq!(decode(&mut bytes.as_bytes().to_vec(), false), Some(key));
    }
    let s = Scene {
        sky: true,
        ..Scene::default()
    };
    let mut v = View::home(&s, 120, 80, 1.);
    let mut flight = Flight {
        roll_keys: [false, true],
        ..Flight::default()
    };
    for _ in 0..60 {
        flight.tick(&mut v, &s, None, 1. / 60.);
    }
    assert!((v.roll - 60.).abs() < 1e-8);
    flight.roll_keys = [false; 2];
    assert!(!flight.tick(&mut v, &s, None, 1.));
}
