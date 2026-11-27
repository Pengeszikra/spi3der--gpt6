use crate::json::{parse,quote,J};
pub type V=[f64;3];
pub fn sub(a:V,b:V)->V{[a[0]-b[0],a[1]-b[1],a[2]-b[2]]}
pub fn cross(a:V,b:V)->V{[a[1]*b[2]-a[2]*b[1],a[2]*b[0]-a[0]*b[2],a[0]*b[1]-a[1]*b[0]]}
pub fn dot(a:V,b:V)->f64{a[0]*b[0]+a[1]*b[1]+a[2]*b[2]}
pub fn norm(a:V)->V{let l=dot(a,a).sqrt().max(1e-15);a.map(|x|x/l)}
pub fn rotate(mut p:V,axis:usize,a:f64)->V{let i=(axis+1)%3;let j=(axis+2)%3;let(s,c)=a.sin_cos();(p[i],p[j])=(p[i]*c-p[j]*s,p[i]*s+p[j]*c);p}
#[derive(Clone,Debug)]pub struct Face{pub v:Vec<usize>,pub color:[u8;3]}
#[derive(Clone,Debug)]pub struct Object{pub name:String,pub p:Vec<V>,pub f:Vec<Face>}
#[derive(Clone,Debug,Default)]pub struct Scene{pub objects:Vec<Object>}
pub fn color(s:&str)->[u8;3]{let s=s.strip_prefix('#').unwrap_or(s);if s.len()==6 && s.is_ascii(){if let Ok(n)=u32::from_str_radix(s,16){return [(n>>16)as u8,(n>>8)as u8,n as u8]}}[180,185,190]}
impl Object{
 pub fn cube(name:String,c:[u8;3])->Self{Self{name,p:vec![[-0.5,0.,-0.5],[0.5,0.,-0.5],[0.5,1.,-0.5],[-0.5,1.,-0.5],[-0.5,0.,0.5],[0.5,0.,0.5],[0.5,1.,0.5],[-0.5,1.,0.5]],f:vec![[4,5,6,7],[1,0,3,2],[1,2,6,5],[0,4,7,3],[3,7,6,2],[0,1,5,4]].into_iter().map(|v|Face{v:v.to_vec(),color:c}).collect()}}
 pub fn transform(&mut self,op:&str,axis:usize,n:f64,ids:&[usize]){let mut c=[0.;3];for &i in ids{for j in 0..3{c[j]+=self.p[i][j]}}c=c.map(|x|x/ids.len().max(1)as f64);for &i in ids{let p=&mut self.p[i];match op{"g"=>p[axis]+=n,"s"=>{for j in 0..3{if j==axis||axis==3{p[j]=c[j]+(p[j]-c[j])*n}}},"r"=>{let q=rotate(sub(*p,c),axis,n.to_radians());*p=[q[0]+c[0],q[1]+c[1],q[2]+c[2]]},_=>{}}}}
 pub fn extrude(&mut self,fi:usize,d:f64)->Result<(),String>{let f=self.f.get(fi).ok_or("face index")?.clone();if f.v.len()<3{return Err("not a polygon".into())}let n=norm(cross(sub(self.p[f.v[1]],self.p[f.v[0]]),sub(self.p[f.v[2]],self.p[f.v[0]])));let start=self.p.len();for &i in &f.v{let p=self.p[i];self.p.push([p[0]+n[0]*d,p[1]+n[1]*d,p[2]+n[2]*d]);}let len=f.v.len();for i in 0..len{let j=(i+1)%len;self.f.push(Face{v:vec![f.v[i],f.v[j],start+j,start+i],color:f.color});}self.f[fi].v=(start..start+len).collect();Ok(())}
}
impl Scene{
 pub fn read(s:&str)->Result<Self,String>{let j=parse(s)?;if j.get("format").str()!="3draft-scene"{return Err("only 3draft-scene supported".into())}if !matches!(j.get("objects"),J::Arr(_)){return Err("missing objects".into())}let pal=j.get("palette").arr();let mut objects=vec![];for o in j.get("objects").arr(){let m=o.get("mesh");if !matches!(m.get("positions"),J::Arr(_)){return Err(format!("unsupported mesh: {}",o.get("name").str()))}let mut p=vec![];for v in m.get("positions").arr(){let a=v.arr();if a.len()!=3||a.iter().any(|n|!matches!(n,J::Num(_))){return Err("invalid vertex".into())}p.push([a[0].num(),a[1].num(),a[2].num()]);}let mut f=vec![];for face in m.get("faces").arr(){let mut v=vec![];for n in face.get("v").arr(){let k=n.num();if !matches!(n,J::Num(_))||k<0.||k.fract()!=0.||k>=p.len()as f64{return Err("invalid face index".into())}v.push(k as usize)}let mat=face.get("mat").str();let c=pal.iter().find(|m|m.get("name").str()==mat).map(|m|m.get("color").str()).unwrap_or(face.get("color").str());if v.len()>=3{f.push(Face{v,color:color(c)})}}objects.push(Object{name:o.get("name").str().into(),p,f});}Ok(Self{objects})}
 pub fn write(&self)->String{let objects=self.objects.iter().map(|o|{let p=o.p.iter().map(|v|format!("[{},{},{}]",v[0],v[1],v[2])).collect::<Vec<_>>().join(",");let f=o.f.iter().map(|f|format!("{{\"v\":[{}],\"color\":\"#{:02x}{:02x}{:02x}\"}}",f.v.iter().map(|n|n.to_string()).collect::<Vec<_>>().join(","),f.color[0],f.color[1],f.color[2])).collect::<Vec<_>>().join(",");format!("{{\"name\":{},\"mesh\":{{\"positions\":[{}],\"faces\":[{}]}}}}",quote(&o.name),p,f)}).collect::<Vec<_>>().join(",");format!("{{\"format\":\"3draft-scene\",\"version\":1,\"palette\":[],\"objects\":[{}]}}",objects)}
}
#[cfg(test)]mod tests{use super::*;#[test]fn roundtrip(){let mut o=Object::cube("cube".into(),[2,3,4]);o.extrude(0,1.).unwrap();let s=Scene{objects:vec![o]};let r=Scene::read(&s.write()).unwrap();assert_eq!(r.objects[0].p.len(),12);assert_eq!(r.objects[0].f.len(),10);}}
