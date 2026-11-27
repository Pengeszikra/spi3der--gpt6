use std::collections::BTreeMap;
#[derive(Clone,Debug)]
pub enum J { Null, Bool, Num(f64), Str(String), Arr(Vec<J>), Obj(BTreeMap<String,J>) }
static NIL:J=J::Null;
impl J {
 pub fn get(&self,k:&str)->&J { if let J::Obj(o)=self {o.get(k).unwrap_or(&NIL)} else {&NIL} }
 pub fn arr(&self)->&[J] {if let J::Arr(a)=self {a} else {&[]} }
 pub fn str(&self)->&str {if let J::Str(s)=self {s} else {""} }
 pub fn num(&self)->f64 {if let J::Num(n)=self {*n} else {0.} }
}
pub fn quote(s:&str)->String {let mut o=String::from("\"");for c in s.chars(){match c {'"'=>o.push_str("\\\""),'\\'=>o.push_str("\\\\"),'\n'=>o.push_str("\\n"),'\r'=>o.push_str("\\r"),'\t'=>o.push_str("\\t"),c if c.is_control()=>o.push_str(&format!("\\u{:04x}",c as u32)),c=>o.push(c)}}o.push('"');o}
pub fn parse(s:&str)->Result<J,String>{let mut p=P{s:s.as_bytes(),i:0};let j=p.val(0)?;p.ws();if p.i!=p.s.len(){return Err("trailing JSON".into())}Ok(j)}
struct P<'a>{s:&'a[u8],i:usize}
impl P<'_>{
 fn ws(&mut self){while self.s.get(self.i).is_some_and(|b|b.is_ascii_whitespace()){self.i+=1}}
 fn take(&mut self,b:u8)->bool{self.ws();if self.s.get(self.i)==Some(&b){self.i+=1;true}else{false}}
 fn string(&mut self)->Result<String,String>{if !self.take(b'"'){return Err("expected string".into())}let mut out=Vec::new();while let Some(&b)=self.s.get(self.i){self.i+=1;match b{b'"'=>return String::from_utf8(out).map_err(|e|e.to_string()),b'\\'=>{let c=*self.s.get(self.i).ok_or("escape")?;self.i+=1;match c{b'u'=>{let h=self.s.get(self.i..self.i+4).ok_or("unicode")?;self.i+=4;let mut n=u32::from_str_radix(std::str::from_utf8(h).map_err(|_|"unicode")?,16).map_err(|_|"unicode")?;if (0xd800..=0xdbff).contains(&n){if self.s.get(self.i..self.i+2)!=Some(b"\\u"){return Err("surrogate".into())}self.i+=2;let h=self.s.get(self.i..self.i+4).ok_or("unicode")?;self.i+=4;let lo=u32::from_str_radix(std::str::from_utf8(h).map_err(|_|"unicode")?,16).map_err(|_|"unicode")?;if !(0xdc00..=0xdfff).contains(&lo){return Err("surrogate".into())}n=0x10000+(n-0xd800)*1024+lo-0xdc00;}let mut buf=[0;4];out.extend_from_slice(char::from_u32(n).ok_or("unicode")?.encode_utf8(&mut buf).as_bytes())},b'n'=>out.push(10),b'r'=>out.push(13),b't'=>out.push(9),b'b'=>out.push(8),b'f'=>out.push(12),b'"'|b'\\'|b'/'=>out.push(c),_=>return Err("bad escape".into())}},0..=31=>return Err("control in string".into()),_=>out.push(b)}}Err("unterminated string".into())}
 fn val(&mut self,d:usize)->Result<J,String>{if d>128{return Err("JSON too deep".into())}self.ws();match self.s.get(self.i).copied(){Some(b'"')=>Ok(J::Str(self.string()?)),Some(b'[')=>{self.i+=1;let mut a=vec![];if !self.take(b']'){loop{a.push(self.val(d+1)?);if self.take(b']'){break}if !self.take(b','){return Err("array comma".into())}}}Ok(J::Arr(a))},Some(b'{')=>{self.i+=1;let mut o=BTreeMap::new();if !self.take(b'}'){loop{let k=self.string()?;if !self.take(b':'){return Err("colon".into())}o.insert(k,self.val(d+1)?);if self.take(b'}'){break}if !self.take(b','){return Err("object comma".into())}}}Ok(J::Obj(o))},Some(b't'|b'f'|b'n')=>{for (s,j) in [("true",J::Bool),("false",J::Bool),("null",J::Null)]{if self.s[self.i..].starts_with(s.as_bytes()){self.i+=s.len();return Ok(j)}}Err("literal".into())},Some(b'-'|b'0'..=b'9')=>{let start=self.i;while self.s.get(self.i).is_some_and(|c|matches!(*c,b'0'..=b'9'|b'-'|b'+'|b'.'|b'e'|b'E')){self.i+=1}let n=std::str::from_utf8(&self.s[start..self.i]).map_err(|_|"number")?.parse::<f64>().map_err(|_|"number")?;if !n.is_finite(){return Err("nonfinite number".into())}Ok(J::Num(n))},_=>Err(format!("unexpected JSON at {}",self.i))}}
}
#[cfg(test)] mod tests{use super::*;#[test]fn strings(){assert_eq!(parse(&quote("árvíz\n🕷")).unwrap().str(),"árvíz\n🕷");assert_eq!(parse(r#""\ud83d\ude00""#).unwrap().str(),"😀");assert!(parse("[1,]").is_err());}}
