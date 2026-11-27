use std::{
    ffi::{c_int, c_ulong, c_void},
    io::{self, Write},
    sync::atomic::{AtomicBool, Ordering},
};
#[repr(C, align(16))]
#[derive(Clone)]
struct Attributes([u8; 256]);
#[repr(C)]
#[derive(Default)]
struct Winsize {
    rows: u16,
    cols: u16,
    width: u16,
    height: u16,
}
#[repr(C)]
struct PollFd {
    fd: c_int,
    events: i16,
    revents: i16,
}
#[cfg(target_os = "macos")]
type Count = u32;
#[cfg(not(target_os = "macos"))]
type Count = c_ulong;
unsafe extern "C" {
    fn tcgetattr(fd: c_int, attr: *mut Attributes) -> c_int;
    fn tcsetattr(fd: c_int, when: c_int, attr: *const Attributes) -> c_int;
    fn cfmakeraw(attr: *mut Attributes);
    fn ioctl(fd: c_int, request: c_ulong, ...) -> c_int;
    fn poll(fds: *mut PollFd, n: Count, ms: c_int) -> c_int;
    fn signal(sig: c_int, handler: usize) -> usize;
}
static STOP: AtomicBool = AtomicBool::new(false);
extern "C" fn stop(_: c_int) {
    STOP.store(true, Ordering::Relaxed)
}
pub struct Terminal {
    saved: Attributes,
    signals: [usize; 3],
}
impl Terminal {
    pub fn open() -> io::Result<Self> {
        let mut saved = Attributes([0; 256]);
        // libc owns the opaque termios layout. The aligned storage exceeds its size on Darwin and Linux.
        unsafe {
            if tcgetattr(0, &mut saved) < 0 {
                return Err(io::Error::last_os_error());
            }
            let mut raw = saved.clone();
            cfmakeraw(&mut raw);
            if tcsetattr(0, 0, &raw) < 0 {
                return Err(io::Error::last_os_error());
            }
        }
        let signals = [1, 2, 15].map(|s| unsafe { signal(s, stop as *const () as usize) });
        let term = Self { saved, signals };
        print!("\x1b[?1049h\x1b[>31u\x1b[?25l\x1b[?1003h\x1b[?1006h\x1b[2J");
        io::stdout().flush()?;
        Ok(term)
    }
    pub fn stopped() -> bool {
        STOP.load(Ordering::Relaxed)
    }
    pub fn ready(ms: i32) -> io::Result<bool> {
        let mut fd = PollFd {
            fd: 0,
            events: 1,
            revents: 0,
        };
        let r = unsafe { poll(&mut fd, 1, ms) };
        if r < 0 {
            let e = io::Error::last_os_error();
            if e.kind() != io::ErrorKind::Interrupted {
                return Err(e);
            }
        }
        Ok(r > 0)
    }
}
impl Drop for Terminal {
    fn drop(&mut self) {
        unsafe {
            tcsetattr(0, 0, &self.saved);
            for (s, h) in [1, 2, 15].into_iter().zip(self.signals) {
                signal(s, h);
            }
        }
        let _ = io::stdout().write_all(b"\x1b[0m\x1b[?1006l\x1b[?1003l\x1b[<u\x1b[?25h\x1b[?1049l");
        let _ = io::stdout().flush();
    }
}
pub fn size() -> (usize, usize, f64) {
    #[cfg(target_os = "macos")]
    const GET: c_ulong = 0x40087468;
    #[cfg(not(target_os = "macos"))]
    const GET: c_ulong = 0x5413;
    let mut s = Winsize::default();
    unsafe {
        ioctl(1, GET, &mut s as *mut Winsize as *mut c_void);
    }
    let aspect = if s.width > 0 && s.height > 0 && s.rows > 0 && s.cols > 0 {
        f64::from(s.height) * f64::from(s.cols) / (2. * f64::from(s.width) * f64::from(s.rows))
    } else {
        1.
    };
    (
        if s.cols == 0 { 80 } else { s.cols as usize }.clamp(10, 600),
        if s.rows == 0 { 24 } else { s.rows as usize }.clamp(5, 300),
        aspect,
    )
}
#[derive(Debug, PartialEq)]
pub enum Key {
    Char(char),
    Repeat(char),
    Roll(char, bool),
    Ignored,
    Ctrl(u8),
    Alt(char),
    Esc,
    Enter,
    ShiftEnter,
    Tab(bool),
    Arrow(char),
    Page(bool),
    Home(bool),
    Back,
    Mouse {
        code: u16,
        x: usize,
        y: usize,
        up: bool,
    },
}
pub fn decode(buf: &mut Vec<u8>, timeout: bool) -> Option<Key> {
    if buf.is_empty() {
        return None;
    }
    let b = buf[0];
    if b == 27 {
        if buf.len() == 1 {
            if !timeout {
                return None;
            }
            buf.remove(0);
            return Some(Key::Esc);
        }
        if buf[1] != b'[' {
            let c = buf[1];
            buf.drain(..2);
            return Some(if c == 13 || c == 10 {
                Key::ShiftEnter
            } else {
                Key::Alt(c as char)
            });
        }
        let Some(i) = buf[2..]
            .iter()
            .position(|b| (64..=126).contains(b))
            .map(|i| i + 2)
        else {
            if timeout || buf.len() > 100 {
                buf.clear()
            }
            return None;
        };
        let seq: Vec<_> = buf.drain(..=i).collect();
        let args = String::from_utf8_lossy(&seq[2..i]);
        let end = seq[i] as char;
        if let Some(xs) = args.strip_prefix('<') {
            let a: Vec<usize> = xs.split(';').filter_map(|s| s.parse().ok()).collect();
            if a.len() == 3 {
                return Some(Key::Mouse {
                    code: a[0] as u16,
                    x: a[1].saturating_sub(1),
                    y: a[2].saturating_sub(1),
                    up: end == 'm',
                });
            }
        }
        let mut fields = args.split(';');
        let codes = fields.next().unwrap_or_default();
        let mut modifiers = fields.next().unwrap_or("1").split(':');
        let flags = modifiers
            .next()
            .and_then(|s| s.parse::<u16>().ok())
            .unwrap_or(1)
            .saturating_sub(1);
        let event = modifiers.next().unwrap_or("1");
        if end == 'u' {
            let mut keys = codes.split(':');
            let cp = |s: &str| s.parse::<u32>().ok().and_then(char::from_u32);
            let Some(base) = keys.next().and_then(cp) else {
                return Some(Key::Ignored);
            };
            if flags & 14 == 0 && matches!(base, '[' | ']' | 'ő' | 'ú') {
                return Some(Key::Roll(base, event != "3"));
            }
            if event == "3" {
                return Some(Key::Ignored);
            }
            let text = fields.next().and_then(|s| s.split(':').next()).and_then(cp);
            let key = match base as u32 {
                27 => Key::Esc,
                13 if flags & 3 != 0 => Key::ShiftEnter,
                13 => Key::Enter,
                9 => Key::Tab(flags & 1 != 0),
                127 | 8 => Key::Back,
                57344..=63743 => Key::Ignored,
                _ if flags & (4 | 8) != 0 && base.is_ascii_alphabetic() => {
                    Key::Ctrl(base.to_ascii_lowercase() as u8 - b'a' + 1)
                }
                _ if flags & 2 != 0 => Key::Alt(base),
                _ => {
                    let ch = text
                        .or_else(|| {
                            if flags & 1 != 0 {
                                keys.next().and_then(cp).or(Some(base.to_ascii_uppercase()))
                            } else {
                                None
                            }
                        })
                        .unwrap_or(base);
                    if event == "2" {
                        Key::Repeat(ch)
                    } else {
                        Key::Char(ch)
                    }
                }
            };
            return Some(key);
        }
        if event == "3" {
            return Some(Key::Ignored);
        }
        return match end {
            'A' | 'B' | 'C' | 'D' => Some(Key::Arrow(end)),
            'Z' => Some(Key::Tab(true)),
            'H' | 'F' => Some(Key::Home(end == 'F')),
            '~' if codes == "5" || codes == "6" => Some(Key::Page(codes == "6")),
            '~' if args == "27;2;13" => Some(Key::ShiftEnter),
            _ => Some(Key::Ignored),
        };
    }
    if b >= 128 {
        let n = match b {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => 1,
        };
        if buf.len() < n && !timeout {
            return None;
        }
        let c = std::str::from_utf8(&buf[..n.min(buf.len())])
            .ok()
            .and_then(|s| s.chars().next())
            .unwrap_or('\u{fffd}');
        buf.drain(..n.min(buf.len()));
        return Some(Key::Char(c));
    }
    buf.remove(0);
    Some(match b {
        10 | 13 => Key::Enter,
        9 => Key::Tab(false),
        127 | 8 => Key::Back,
        0..=31 => Key::Ctrl(b),
        _ => Key::Char(b as char),
    })
}
