pub fn json(s:&str)->String{
 let mut out=String::new();let mut it=s.chars().peekable();
 while let Some(c)=it.next(){match c{
 '"'=>{let mut t=String::from("\"");while let Some(x)=it.next(){t.push(x);if x=='\\'{if let Some(y)=it.next(){t.push(y)}}else if x=='"'{break}}let key=it.clone().find(|x|!x.is_whitespace())==Some(':');out.push_str(if key{"\x1b[38;2;225;164;204m"}else{"\x1b[38;2;55;210;140m"});out.push_str(&t);out.push_str("\x1b[0m");},
 '0'..='9'|'-'=>{out.push_str("\x1b[38;2;255;176;95m");out.push(c);while it.peek().is_some_and(|x|matches!(x,'0'..='9'|'.'|'e'|'E'|'+'|'-')){out.push(it.next().unwrap())}out.push_str("\x1b[0m");},
 '{'|'}'|'['|']'=>{out.push_str("\x1b[38;2;255;145;45m");out.push(c);out.push_str("\x1b[0m");},
 c if c.is_control()&&c!='\n'&&c!='\t'=>{},_=>out.push(c)
 }}out
}
#[cfg(test)]mod tests{use super::*;#[test]fn colors(){let s=json(r#"{"x":12,"y":"a"}"#);assert!(s.contains("225;164;204"));assert!(s.contains("55;210;140"));assert!(s.contains("255;176;95"));}}
