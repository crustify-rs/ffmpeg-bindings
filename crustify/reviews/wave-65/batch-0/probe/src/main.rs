//! Differential probe: random op sequences on a C `AVBPrint` and a Rust
//! `BPrint` of the same shape, full state compared after every step.
#![allow(clippy::all)]
use core::ffi::{CStr, c_char, c_uint};
use core::mem::MaybeUninit;
use core::ptr::{self, addr_of, addr_of_mut};
use std::ffi::CString;

use ffmpeg::libavutil::libavutil::avstring::{EscapeFlags, EscapeMode};
use ffmpeg::libavutil::libavutil::bprint::{BPrint, BPrintMut, BPrintRef};
use glibc_bindings::libc::time::bits::types::struct_tm_h::Tm;
use ffmpeg::libavutil::libavutil::ffi;
use ffmpeg::libavutil::libavutil::mem::set_max_alloc;

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    fn below(&mut self, n: u64) -> u64 {
        self.next() % n
    }
    fn pick<T: Copy>(&mut self, xs: &[T]) -> T {
        xs[self.below(xs.len() as u64) as usize]
    }
}

struct CBuf {
    buf: *mut ffi::AVBPrint,
    owned: bool,
}
impl CBuf {
    fn storage() -> *mut ffi::AVBPrint {
        Box::into_raw(Box::new(MaybeUninit::<ffi::AVBPrint>::uninit())).cast()
    }
    fn init(size_init: c_uint, size_max: c_uint) -> Self {
        let buf = Self::storage();
        unsafe { ffi::av_bprint_init(buf, size_init, size_max) };
        Self { buf, owned: true }
    }
    fn for_buffer(data: &mut [u8]) -> Self {
        let buf = Self::storage();
        unsafe { ffi::av_bprint_init_for_buffer(buf, data.as_mut_ptr().cast(), data.len() as c_uint) };
        Self { buf, owned: false }
    }
    fn state(&self) -> State {
        let b = self.buf;
        unsafe {
            let str = addr_of!((*b).str_).read();
            let len = addr_of!((*b).len).read();
            let size = addr_of!((*b).size).read();
            let stored = len.min(size.saturating_sub(1)) as usize;
            let bytes = if stored == 0 { Vec::new() } else { core::slice::from_raw_parts(str.cast::<u8>(), stored).to_vec() };
            let c_str = if size == 0 { Vec::new() } else { CStr::from_ptr(str).to_bytes().to_vec() };
            State {
                len,
                size,
                size_max: addr_of!((*b).size_max).read(),
                inline: ptr::eq(str, addr_of_mut!((*b).reserved_internal_buffer).cast()),
                complete: ffi::av_bprint_is_complete(b) != 0,
                bytes,
                c_str,
            }
        }
    }
    fn finalize(mut self) -> (i32, Option<Vec<u8>>) {
        let mut str = ptr::null_mut();
        let ret = unsafe { ffi::av_bprint_finalize(self.buf, &mut str) };
        let real = unsafe { addr_of!((*self.buf).size).read() } as usize;
        let bytes = (ret == 0 && real > 0).then(|| unsafe { core::slice::from_raw_parts(str.cast::<u8>(), real - 1) }.to_vec());
        unsafe { ffi::av_free(str.cast()) };
        self.owned = false;
        (ret, bytes)
    }
}
impl Drop for CBuf {
    fn drop(&mut self) {
        if self.owned {
            unsafe { ffi::av_bprint_finalize(self.buf, ptr::null_mut()) };
        }
        drop(unsafe { Box::from_raw(self.buf.cast::<MaybeUninit<ffi::AVBPrint>>()) });
    }
}

#[derive(Debug, PartialEq)]
struct State {
    len: u32,
    size: u32,
    size_max: u32,
    inline: bool,
    complete: bool,
    bytes: Vec<u8>,
    c_str: Vec<u8>,
}
fn rust_state(r: BPrintRef<'_>) -> State {
    State {
        len: r.len(),
        size: r.size(),
        size_max: r.size_max(),
        inline: r.is_inline(),
        complete: r.is_complete(),
        bytes: r.as_bytes().to_vec(),
        c_str: r.as_c_str().to_bytes().to_vec(),
    }
}

fn c_tm() -> glibc_bindings::libc::ffi::tm {
    glibc_bindings::libc::ffi::tm { tm_sec: 9, tm_min: 8, tm_hour: 7, tm_mday: 20, tm_mon: 11, tm_year: 100, tm_wday: 3, tm_yday: 354, tm_isdst: 0, tm_gmtoff: 5400, tm_zone: c"XST".as_ptr() }
}
fn rust_tm() -> Tm {
    let mut tm = Tm::new();
    let mut m = tm.as_mut();
    m.set_sec(9); m.set_min(8); m.set_hour(7); m.set_mday(20); m.set_mon(11); m.set_year(100);
    m.set_wday(3); m.set_yday(354); m.set_isdst(0); m.set_gmtoff(5400); m.set_zone(Some(c"XST"));
    tm
}

#[derive(Clone, Debug)]
enum Op {
    Append(Vec<u8>),
    Chars(u8, u32),
    Clear,
    Truncate(u32),
    Fmt(i32, CString),
    CStr(CString),
    Escape(CString, Option<CString>, EscapeMode, EscapeFlags),
    Strftime(CString),
    GetBuffer(u32, Vec<u8>),
    MaxAlloc(usize),
}

fn c_op(b: *mut ffi::AVBPrint, op: &Op) -> Option<usize> {
    unsafe {
        match op {
            Op::Append(d) => ffi::av_bprint_append_data(b, d.as_ptr().cast(), d.len() as c_uint),
            Op::Chars(c, n) => ffi::av_bprint_chars(b, *c as c_char, *n),
            Op::Clear => ffi::av_bprint_clear(b),
            Op::Truncate(n) => {
                if *n < (*b).len {
                    (*b).len = *n;
                    if (*b).size > 0 {
                        *(*b).str_.add((*n).min((*b).size - 1) as usize) = 0;
                    }
                }
            }
            Op::Fmt(i, s) => ffi::av_bprintf(b, c"%d-%s".as_ptr(), *i, s.as_ptr()),
            Op::CStr(s) => ffi::crustify_av_vbprintf(b, c"%s".as_ptr(), s.as_ptr()),
            Op::Escape(src, special, mode, flags) => ffi::av_bprint_escape(b, src.as_ptr(), special.as_ref().map_or(ptr::null(), |s| s.as_ptr()), (*mode).into(), flags.bits()),
            Op::Strftime(fmt) => ffi::av_bprint_strftime(b, fmt.as_ptr(), std::ptr::from_ref(&c_tm()).cast()),
            Op::GetBuffer(size, data) => {
                let mut mem = ptr::null_mut();
                let mut room = 0;
                ffi::av_bprint_get_buffer(b, *size, &mut mem, &mut room);
                let spare = room.saturating_sub(1).min(*size) as usize;
                if spare > 0 {
                    ptr::write_bytes(mem, 0, spare);
                }
                let n = data.len().min(spare);
                if n > 0 {
                    ptr::copy_nonoverlapping(data.as_ptr(), mem, n);
                    (*b).len += n as c_uint;
                    *(*b).str_.add((*b).len as usize) = 0;
                }
                return Some(n);
            }
            Op::MaxAlloc(n) => set_max_alloc(*n),
        }
    }
    None
}

fn rust_op(m: &mut BPrintMut<'_>, op: &Op) -> Option<usize> {
    match op {
        Op::Append(d) => m.append_bytes(d),
        Op::Chars(c, n) => m.append_repeated(*c, *n),
        Op::Clear => m.clear(),
        Op::Truncate(n) => m.truncate(*n),
        Op::Fmt(i, s) => m.append_fmt(format_args!("{}-{}", i, s.to_str().unwrap())),
        Op::CStr(s) => m.append_c_str(s),
        Op::Escape(src, special, mode, flags) => m.append_escaped(src, special.as_deref(), *mode, *flags),
        Op::Strftime(fmt) => m.append_strftime(fmt, rust_tm().as_ref()),
        Op::GetBuffer(size, data) => {
            return Some(m.append_with(*size, |spare| {
                let n = data.len().min(spare.len());
                spare[..n].copy_from_slice(&data[..n]);
                n
            }));
        }
        Op::MaxAlloc(_) => {}
    }
    None
}

const SIZES: &[u32] = &[0, 1, 2, 3, 7, 8, 30, 100, 1000, 1003, 1004, 1005, 1024, 1500, 2000, 2048, 3000, 5000, 20_000];
const FMTS: &[&CStr] = &[c"%Y-%m-%d", c"%c", c"", c"%p %Z %z", c"%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c|%c", c"%2000c", c"%Z", c"%500c", c"x", c"%%%%"];
const ESC_SRC: &[&[u8]] = &[b"", b" ", b"\tlead and trail\n", b"a'b\\c\"d<e>f&g:h\ri j", b"<]]>'\"'\"&amp;", b"'", b"\\\\", b"a b", b"\n"];

fn random_bytes(rng: &mut Rng, max: usize) -> Vec<u8> {
    let n = rng.below(max as u64 + 1) as usize;
    (0..n).map(|_| rng.pick(&[0u8, 1, b'a', b'z', 0xff, b' ', b'\n', b'\\', b'\'', b'"', b'<', b'&'])).collect()
}
fn random_cstring(rng: &mut Rng, max: usize) -> CString {
    let n = rng.below(max as u64 + 1) as usize;
    let v: Vec<u8> = (0..n).map(|_| rng.pick(&[b'a', b'z', 0xff, b' ', b'\n', b'\\', b'\'', b'"', b'<', b'&', b'%', b':'])).collect();
    CString::new(v).unwrap()
}

fn random_op(rng: &mut Rng) -> Op {
    match rng.below(14) {
        0 | 1 => Op::Append(random_bytes(rng, 3000)),
        2 | 3 => Op::Chars(rng.pick(&[0u8, b'x', 0xff, b' ']), rng.pick(&[0, 1, 2, 5, 100, 1003, 1004, 1005, 2000, 5000])),
        4 => Op::Clear,
        5 => Op::Truncate(rng.pick(&[0, 1, 2, 5, 100, 999, 1003, 1004, 1500, 3000, 10_000, u32::MAX])),
        6 => Op::Fmt(rng.next() as i32, { let mut c = random_cstring(rng, 40).into_bytes(); c.retain(|b| b.is_ascii()); CString::new(c).unwrap() }),
        7 => Op::CStr(random_cstring(rng, 2000)),
        8 => {
            let src = CString::new(rng.pick(ESC_SRC)).unwrap();
            let special = match rng.below(3) { 0 => None, 1 => Some(CString::new("").unwrap()), _ => Some(random_cstring(rng, 4)) };
            let mode = rng.pick(&[EscapeMode::Auto, EscapeMode::Backslash, EscapeMode::Quote, EscapeMode::Xml]);
            let flags = EscapeFlags::from_bits_retain(rng.below(16) as i32);
            Op::Escape(src, special, mode, flags)
        }
        9 | 10 => Op::Strftime(CString::from(rng.pick(FMTS))),
        11 | 12 => {
            let size = rng.pick(&[0, 1, 2, 5, 100, 1000, 1003, 1004, 1005, 2000, 5000, u32::MAX]);
            Op::GetBuffer(size, random_bytes(rng, 6000))
        }
        _ => Op::MaxAlloc(rng.pick(&[0, 50, 100, 1024, 1100, 2047, 4096, i32::MAX as usize])),
    }
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map(|s| s.parse().unwrap()).unwrap_or(20_000);
    let seed: u64 = std::env::args().nth(2).map(|s| s.parse().unwrap()).unwrap_or(0x9e3779b97f4a7c15);
    let mut rng = Rng(seed);
    let mut failures = 0;
    for it in 0..iters {
        set_max_alloc(i32::MAX as usize);
        let shape = rng.below(3);
        let nops = 1 + rng.below(25) as usize;
        let ops: Vec<Op> = (0..nops).map(|_| random_op(&mut rng)).collect();
        let (size_init, size_max) = (rng.pick(SIZES), rng.pick(&[0u32, 1, 2, 3, 8, 30, 1000, 1003, 1004, 1005, 1500, 3000, 5000, u32::MAX]));
        let blen = rng.pick(&[0usize, 1, 2, 5, 64, 1004, 1100, 2048, 5000]);
        let mut c_data = vec![0x5a_u8; blen];
        let mut r_data = c_data.clone();
        let ctx = if shape == 2 { format!("for_buffer({blen})") } else { format!("init({size_init}, {size_max})") };
        let mut ok = true;
        if shape == 2 {
            let c = CBuf::for_buffer(&mut c_data);
            let mut r = BPrint::for_buffer(&mut r_data).unwrap();
            for (i, op) in ops.iter().enumerate() {
                let n = c_op(c.buf, op);
                let m = rust_op(&mut r.as_mut(), op);
                let (cs, rs) = (c.state(), rust_state(r.as_ref()));
                if n != m || cs != rs {
                    println!("MISMATCH it={it} {ctx} step {i} {op:?}\n  ret C {n:?} R {m:?}\n  C {cs:?}\n  R {rs:?}");
                    ok = false;
                    break;
                }
            }
            drop(r);
            drop(c);
            if ok && r_data != c_data {
                println!("MISMATCH it={it} {ctx} caller buffers differ after drop");
                ok = false;
            }
        } else {
            let c = CBuf::init(size_init, size_max);
            let mut r = BPrint::new(size_init, size_max).unwrap();
            for (i, op) in ops.iter().enumerate() {
                let n = c_op(c.buf, op);
                let m = rust_op(&mut r.as_mut(), op);
                let (cs, rs) = (c.state(), rust_state(r.as_ref()));
                if n != m || cs != rs {
                    println!("MISMATCH it={it} {ctx} step {i} {op:?}\n  ret C {n:?} R {m:?}\n  C {cs:?}\n  R {rs:?}");
                    ok = false;
                    break;
                }
            }
            if ok {
                let expect = c.state();
                let (ret, c_bytes) = c.finalize();
                if shape == 0 {
                    match BPrint::into_bytes(r) {
                        Ok(b) => {
                            if ret != 0 || b.as_slice() != c_bytes.as_deref().unwrap_or_default() {
                                println!("MISMATCH it={it} {ctx} into_bytes: C ret {ret} {c_bytes:?} vs R {:?}", b.as_slice());
                                ok = false;
                            }
                        }
                        Err(e) => {
                            if ret != e {
                                println!("MISMATCH it={it} {ctx} into_bytes err {e} vs C ret {ret}");
                                ok = false;
                            }
                        }
                    }
                } else {
                    match BPrint::into_string(r) {
                        Ok(s) => {
                            let want = match &c_bytes {
                                Some(b) => CStr::from_bytes_until_nul(&[&b[..], b"\0"].concat()).unwrap().to_bytes().to_vec(),
                                None => { if expect.size != 0 { println!("MISMATCH it={it} C no string but size {}", expect.size); ok = false; } Vec::new() }
                            };
                            if ret != 0 || s.as_bytes() != want {
                                println!("MISMATCH it={it} {ctx} into_string: C ret {ret} {want:?} vs R {:?}", s.as_bytes());
                                ok = false;
                            }
                        }
                        Err(e) => {
                            // Rust's count-only "" needs a 1-byte alloc where C's av_malloc(0) also needs one.
                            if ret != e {
                                println!("MISMATCH it={it} {ctx} into_string err {e} vs C ret {ret}");
                                ok = false;
                            }
                        }
                    }
                }
            }
        }
        if !ok {
            failures += 1;
            if failures > 10 { break; }
        }
    }
    set_max_alloc(i32::MAX as usize);
    println!("done: {iters} iterations, {failures} failures");
    std::process::exit(if failures == 0 { 0 } else { 1 });
}
