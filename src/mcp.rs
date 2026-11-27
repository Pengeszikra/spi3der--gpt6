use crate::{App,Scene,json::{J,parse,quote}};
use std::io::{self,BufRead,Write};
fn id(j:&J)->String{match j{J::Num(n)=>n.to_string(),J::Str(s)=>quote(s),_=>"null".into()}}
fn result(i:&str,s:&str)->String{format!(r#"{{"jsonrpc":"2.0","id":{i},"result":{s}}}"#)}
fn error(i:&str,c:i32,s:&str)->String{format!(r#"{{"jsonrpc":"2.0","id":{i},"error":{{"code":{c},"message":{}}}}}"#,quote(s))}
const TOOLS:&str=r#"{"tools":[{"name":"scene_list","description":"List objects in the current MCP scene","inputSchema":{"type":"object","properties":{}}},{"name":"scene_command","description":"Run a native editor command, e.g. /add box, /g x 2, /r y 45, /save name, /load name, /new, /ls. The MCP scene is separate from the TUI.","inputSchema":{"type":"object","properties":{"command":{"type":"string"}},"required":["command"]}},{"name":"scene_export","description":"Return current scene JSON","inputSchema":{"type":"object","properties":{}}}]}"#;
fn dispatch(a:&mut App,line:&str)->Option<String>{
 let j=match parse(line){Ok(j)=>j,Err(e)=>return Some(error("null",-32700,&e))};
 let method=j.get("method").str();let i=id(j.get("id"));
 if matches!(j.get("id"),J::Null){return None}
 let p=j.get("params");
 Some(match method{
 "initialize"=>{let v=p.get("protocolVersion").str();let v=if ["2024-11-05","2025-03-26","2025-06-18","2025-11-25"].contains(&v){v}else{"2024-11-05"};result(&i,&format!(r#"{{"protocolVersion":{},"capabilities":{{"tools":{{}}}},"serverInfo":{{"name":"spi3derr","version":{}}}}}"#,quote(v),quote(crate::VERSION)))},
 "ping"=>result(&i,"{}"),
 "tools/list"=>result(&i,TOOLS),
 "tools/call"=>{let args=p.get("arguments");let r=match p.get("name").str(){
 "scene_list"=>Ok(a.scene.objects.iter().map(|o|o.name.as_str()).collect::<Vec<_>>().join("\n")),
 "scene_export"=>Ok(a.scene.write()),
 "scene_command"=>{let cmd=args.get("command").str();if cmd.is_empty(){Err("command required".into())}else if matches!(cmd.split_whitespace().next(),Some("/q"|"/quit")){Err("quit is not an MCP command".into())}else{a.note.clear();a.command(cmd).map(|_|if a.note.is_empty(){"OK".into()}else{a.note.clone()})}},
 _=>Err("unknown tool".into())};
 let (s,e)=match r{Ok(s)=>(s,false),Err(s)=>(s,true)};
 result(&i,&format!(r#"{{"content":[{{"type":"text","text":{}}}],"isError":{e}}}"#,quote(&s)))},
 _=>error(&i,-32601,"method not found")})
}
pub fn run()->Result<(),String>{let mut a=App::new(Scene::default());let stdin=io::stdin();let mut out=io::stdout().lock();for l in stdin.lock().lines(){let l=l.map_err(|e|e.to_string())?;if let Some(r)=dispatch(&mut a,&l){writeln!(out,"{r}").and_then(|_|out.flush()).map_err(|e|e.to_string())?}}Ok(())}
#[cfg(test)]mod tests{use super::*;#[test]fn rpc(){let mut a=App::new(Scene::default());let r=dispatch(&mut a,r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"scene_command","arguments":{"command":"/add box"}}}"#).unwrap();assert!(parse(&r).is_ok());assert_eq!(a.scene.objects.len(),1);assert!(dispatch(&mut a,r#"{"jsonrpc":"2.0","method":"notifications/initialized"}"#).is_none());}}
