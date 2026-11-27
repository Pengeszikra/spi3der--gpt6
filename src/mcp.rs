use crate::{
    geometry::*,
    json::{parse, quote, J},
    scene::*,
    App, Scene, Session,
};
use std::io::{self, BufRead, Write};
fn id(j: &J) -> String {
    match j {
        J::Num(n) => n.to_string(),
        J::Str(s) => quote(s),
        _ => "null".into(),
    }
}
fn result(i: &str, s: &str) -> String {
    format!(r#"{{"jsonrpc":"2.0","id":{i},"result":{s}}}"#)
}
fn error(i: &str, c: i32, s: &str) -> String {
    format!(
        r#"{{"jsonrpc":"2.0","id":{i},"error":{{"code":{c},"message":{}}}}}"#,
        quote(s)
    )
}
fn surface(a: &App, args: &J) -> Result<Face, String> {
    let name = args.get("material").str();
    if name.is_empty() {
        return Ok(a.surface());
    }
    let m = a
        .scene
        .materials()
        .into_iter()
        .find(|m| m.get("name").str() == name)
        .ok_or("unknown material")?;
    Ok(Face {
        mat: name.into(),
        color: color(m.get("color").str()),
        emit: m.get("emit").num(),
        ..Face::default()
    })
}
fn build(a: &mut App, tool: &str, args: &J) -> Result<String, String> {
    let mut out = vec![];
    let items = if tool == "add_primitives" {
        args.get("objects").arr()
    } else {
        std::slice::from_ref(args)
    };
    if items.is_empty() {
        return Err("empty batch".into());
    }
    for item in items {
        let name = item.get("name").str();
        if name.trim().is_empty() || a.scene.objects.iter().chain(&out).any(|o| o.name == name) {
            return Err(format!("missing or duplicate name {name}"));
        }
        let surf = surface(a, item)?;
        let mut o = match tool {
            "add_primitives" => {
                let center = vec3(item.get("center")).ok_or("center needs 3 numbers")?;
                let size = vec3(item.get("size")).ok_or("size needs 3 numbers")?;
                if size.iter().any(|v| *v <= 0.) {
                    return Err("size must be positive".into());
                }
                let mut o = primitive(item.get("shape").str(), name, &surf)?;
                let mut lo = [f64::INFINITY; 3];
                let mut hi = [f64::NEG_INFINITY; 3];
                for p in &o.p {
                    for i in 0..3 {
                        lo[i] = lo[i].min(p[i]);
                        hi[i] = hi[i].max(p[i])
                    }
                }
                for p in &mut o.p {
                    for i in 0..3 {
                        p[i] = center[i] + ((p[i] - lo[i]) / (hi[i] - lo[i]) - 0.5) * size[i]
                    }
                }
                o
            }
            "add_mesh" => {
                let p = item
                    .get("positions")
                    .arr()
                    .iter()
                    .map(|v| vec3(v).ok_or("invalid position"))
                    .collect::<Result<Vec<_>, _>>()?;
                let mut o = Object {
                    name: name.into(),
                    p,
                    ..Object::default()
                };
                for f in item.get("faces").arr() {
                    let mut v = vec![];
                    for i in f.arr() {
                        let k = i.number().ok_or("invalid index")?;
                        if k.fract() != 0. || k < 0. || k >= o.p.len() as f64 {
                            return Err("invalid index".into());
                        }
                        v.push(k as usize)
                    }
                    if v.len() < 3 {
                        return Err("face needs three vertices".into());
                    }
                    o.f.push(Face { v, ..surf.clone() })
                }
                if o.f.is_empty() {
                    return Err("mesh needs faces".into());
                }
                o
            }
            _ => {
                let origin = vec3(item.get("origin")).ok_or("expected origin")?;
                let mut profile = vec![];
                for p in item.get("profile").arr() {
                    let [u, r] = p.arr() else {
                        return Err("expected height/radius pairs".into());
                    };
                    profile.push([
                        u.number().ok_or("invalid height")?,
                        r.number().ok_or("invalid radius")?,
                    ]);
                }
                let n = item.get("segments").number().unwrap_or(24.);
                if n.fract() != 0. || !(4. ..=512.).contains(&n) {
                    return Err("invalid segment count".into());
                }
                lathe(
                    name,
                    &profile,
                    origin,
                    [0., 1., 0.],
                    [1., 0., 0.],
                    n as usize,
                    &surf,
                )?
            }
        };
        o.refresh();
        out.push(o)
    }
    let names = out
        .iter()
        .map(|o| o.name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    a.snapshot();
    a.scene.objects.extend(out);
    Ok(names)
}
fn dispatch(a: &mut App, line: &str, mut session: Option<&mut Session>) -> Option<String> {
    let j = match parse(line) {
        Ok(j) => j,
        Err(e) => return Some(error("null", -32700, &e)),
    };
    let method = j.get("method").str();
    let i = id(j.get("id"));
    if matches!(j.get("id"), J::Null) {
        return None;
    }
    let p = j.get("params");
    Some(match method {
        "initialize" => {
            let v = p.get("protocolVersion").str();
            let v = if ["2024-11-05", "2025-03-26", "2025-06-18", "2025-11-25"].contains(&v) {
                v
            } else {
                "2024-11-05"
            };
            result(
                &i,
                &format!(
                    r#"{{"protocolVersion":{},"capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"spi3derr","version":{}}}}}"#,
                    quote(v),
                    quote(crate::VERSION)
                ),
            )
        }
        "ping" => result(&i, "{}"),
        "tools/list" => result(
            &i,
            &include_str!("tools.json")
                .lines()
                .map(str::trim)
                .collect::<String>(),
        ),
        "tools/call" => {
            let args = p.get("arguments");
            let r = (|| -> Result<String, String> {
                if let Some(s) = session.as_deref_mut() {
                    if let Some(scene) = s.refresh()? {
                        a.reload(scene)
                    }
                    let expected = args.get("revision").str();
                    if !expected.is_empty() && s.revision() != expected {
                        return Err("scene revision conflict".into());
                    }
                }
                let generation = a.generation;
                let answer = match p.get("name").str() {
                    "add_mesh" | "add_primitives" | "lathe_profile" => {
                        build(a, p.get("name").str(), args)
                    }
                    "scene_state" => {
                        let mut state = J::object();
                        state.put(
                            "revision",
                            J::Str(session.as_ref().map_or(String::new(), |s| s.revision())),
                        );
                        state.put(
                            "objects",
                            J::Arr(
                                a.scene
                                    .objects
                                    .iter()
                                    .map(|o| {
                                        let mut data = J::object();
                                        data.put("name", J::Str(o.name.clone()));
                                        data.put("center", jvec(o.center));
                                        data.put("radius", J::Num(o.radius));
                                        data
                                    })
                                    .collect(),
                            ),
                        );
                        Ok(state.encode())
                    }
                    "scene_list" => Ok(a
                        .scene
                        .objects
                        .iter()
                        .map(|o| o.name.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")),
                    "scene_export" => Ok(a.scene.write()),
                    "scene_command" => {
                        let cmd = args.get("command").str();
                        if cmd.is_empty() {
                            Err("command required".into())
                        } else if matches!(cmd.split_whitespace().next(), Some("/q" | "/quit")) {
                            Err("quit is not an MCP command".into())
                        } else {
                            a.note.clear();
                            a.command(cmd).map(|_| {
                                if a.note.is_empty() {
                                    "OK".into()
                                } else {
                                    a.note.clone()
                                }
                            })
                        }
                    }
                    _ => Err("unknown tool".into()),
                }?;
                if a.generation != generation {
                    if let Some(s) = session.as_mut() {
                        if let Err(e) = s.publish(&a.scene) {
                            a.reload(s.last_valid());
                            return Err(e);
                        }
                    }
                }
                Ok(answer)
            })();
            let (s, e) = match r {
                Ok(s) => (s, false),
                Err(s) => (s, true),
            };
            result(
                &i,
                &format!(
                    r#"{{"content":[{{"type":"text","text":{}}}],"isError":{e}}}"#,
                    quote(&s)
                ),
            )
        }
        _ => error(&i, -32601, "method not found"),
    })
}
pub fn run(path: Option<std::path::PathBuf>) -> Result<(), String> {
    let (mut session, scene) = Session::open(
        path.unwrap_or_else(Session::default_path),
        &Scene::default(),
    )?;
    let mut a = App::new(scene);
    a.shared = session.path.canonicalize().ok();
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for l in stdin.lock().lines() {
        let l = l.map_err(|e| e.to_string())?;
        if let Some(r) = dispatch(&mut a, &l, Some(&mut session)) {
            writeln!(out, "{r}")
                .and_then(|_| out.flush())
                .map_err(|e| e.to_string())?
        }
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rpc() {
        let mut a = App::new(Scene::default());
        let r=dispatch(&mut a,r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_command","arguments":{"command":"/add box"}}}"#,None).unwrap();
        assert!(parse(&r).is_ok());
        assert_eq!(a.scene.objects.len(), 1);
        assert!(dispatch(
            &mut a,
            r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#,
            None
        )
        .is_none());
    }
}
