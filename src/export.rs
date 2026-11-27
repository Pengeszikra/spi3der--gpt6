use crate::{
    camera::View,
    render::{render, Frame},
    scene::*,
};
use std::fmt::Write;
fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    out.extend(tag);
    out.extend(data);
    let mut crc = !0u32;
    for &b in tag.iter().chain(data) {
        crc ^= u32::from(b);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xedb88320u32 & 0u32.wrapping_sub(crc & 1));
        }
    }
    out.extend((!crc).to_be_bytes());
}
pub fn png(f: &Frame) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut header = (f.w as u32).to_be_bytes().to_vec();
    header.extend((f.h as u32).to_be_bytes());
    header.extend([8, 6, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &header);
    let mut raw = vec![];
    for y in 0..f.h {
        raw.push(0);
        for x in 0..f.w {
            let i = y * f.w + x;
            raw.extend(f.rgb[i]);
            raw.push(if f.z[i].is_finite() || f.rgb[i] != [12, 14, 18] {
                255
            } else {
                0
            })
        }
    }
    let (mut a, mut b) = (1u32, 0u32);
    let mut z = vec![0x78, 1];
    let chunks = raw.chunks(65535);
    let n = chunks.len();
    for (i, block) in chunks.enumerate() {
        let len = block.len() as u16;
        z.push(u8::from(i + 1 == n));
        z.extend(len.to_le_bytes());
        z.extend((!len).to_le_bytes());
        z.extend(block);
        for &v in block {
            a = (a + u32::from(v)) % 65521;
            b = (b + a) % 65521
        }
    }
    z.extend(((b << 16) | a).to_be_bytes());
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}
fn paths(s: &Scene, v: &View, w: usize, h: usize) -> String {
    let c = v.camera(w, h);
    let mut shapes = vec![];
    let light = norm(add(mul(c.f, -0.55), add(mul(c.r, -0.35), mul(c.u, 0.75))));
    for o in &s.objects {
        if let Some(star) = &o.star {
            let q = c.project(o.center);
            if q[2] > 0. && q[0] >= 0. && q[0] < w as f64 && q[1] >= 0. && q[1] < h as f64 {
                let [r, g, b] = star.rgb.map(|v| (v as f64 * star.brightness).round() as u8);
                shapes.push((q[2],format!("<circle cx=\"{:.2}\" cy=\"{:.2}\" r=\"{}\" fill=\"#{r:02x}{g:02x}{b:02x}\"/>",q[0],q[1],if star.big{1.5}else{0.6})))
            }
            continue;
        }
        let p: Vec<_> = o.p.iter().map(|&p| c.project(p)).collect();
        for f in &o.f {
            if f.v.len() < 3 || f.v.iter().any(|&i| p[i][2] <= 1e-12) {
                continue;
            }
            let n = if dot(f.normal, sub(c.eye, f.mid)) < 0. {
                mul(f.normal, -1.)
            } else {
                f.normal
            };
            let shade = (0.32 + 0.85 * dot(n, light).max(0.) + 0.4 * f.emit).min(1.25);
            let [r, g, b] = f.color.map(|x| (x as f64 * shade).round() as u8);
            let mut shape = format!("<polygon fill=\"#{r:02x}{g:02x}{b:02x}\" points=\"");
            for &i in &f.v {
                write!(shape, "{:.2},{:.2} ", p[i][0], p[i][1]).unwrap()
            }
            shape.push_str("\"/>");
            shapes.push((c.project(f.mid)[2], shape))
        }
    }
    shapes.sort_by(|a, b| b.0.total_cmp(&a.0));
    shapes.into_iter().map(|s| s.1).collect()
}
pub fn svg(s: &Scene, v: &View, w: usize, h: usize, frames: usize, seconds: f64) -> String {
    let mut out = format!("<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 {w} {h}\">");
    for i in 0..frames {
        let mut view = v.clone();
        view.yaw += 360. * i as f64 / frames as f64;
        if frames > 1 {
            let tick = seconds / frames as f64;
            write!(out,"<g visibility=\"hidden\"><set attributeName=\"visibility\" to=\"visible\" begin=\"{:.4}s;f{i}.end+{:.4}s\" dur=\"{tick:.4}s\" id=\"f{i}\"/>",i as f64*tick,seconds-tick).unwrap()
        }
        out.push_str(&paths(s, &view, w, h));
        if frames > 1 {
            out.push_str("</g>")
        }
    }
    out.push_str("</svg>\n");
    out
}
pub fn gif(s: &Scene, v: &View, w: usize, h: usize, frames: usize, seconds: f64) -> Vec<u8> {
    let mut out = b"GIF89a".to_vec();
    out.extend((w as u16).to_le_bytes());
    out.extend((h as u16).to_le_bytes());
    out.extend([0xf7, 0, 0]);
    for i in 0..256 {
        out.extend([
            ((i >> 5) * 255 / 7) as u8,
            (((i >> 2) & 7) * 255 / 7) as u8,
            ((i & 3) * 255 / 3) as u8,
        ])
    }
    out.extend(b"\x21\xff\x0bNETSCAPE2.0\x03\x01\x00\x00\x00");
    let delay = (100. * seconds / frames as f64).round().clamp(1., 65535.) as u16;
    for i in 0..frames {
        let mut view = v.clone();
        view.yaw += 360. * i as f64 / frames as f64;
        let f = render(s, &view, w, h, None, None);
        out.extend([0x21, 0xf9, 4, 0]);
        out.extend(delay.to_le_bytes());
        out.extend([0, 0, 0x2c, 0, 0, 0, 0]);
        out.extend((w as u16).to_le_bytes());
        out.extend((h as u16).to_le_bytes());
        out.extend([0, 8]);
        // Nine-bit literal runs with periodic LZW clear codes. Reset before the dictionary would grow to ten bits.
        let mut bits = 0u32;
        let mut available = 0;
        let mut packed = vec![];
        let mut emit = |code: u32| {
            bits |= code << available;
            available += 9;
            while available >= 8 {
                packed.push(bits as u8);
                bits >>= 8;
                available -= 8;
            }
        };
        for run in f.rgb.chunks(250) {
            emit(256);
            for &[r, g, b] in run {
                emit(u32::from((r & 0xe0) | ((g >> 3) & 0x1c) | (b >> 6)))
            }
        }
        emit(257);
        if available > 0 {
            packed.push(bits as u8)
        }
        for block in packed.chunks(255) {
            out.push(block.len() as u8);
            out.extend(block)
        }
        out.push(0)
    }
    out.push(0x3b);
    out
}
