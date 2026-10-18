//! Differential probe of `pixdesc::read_image_line` / `write_image_line`:
//! an independent, literal transliteration of C's `av_read_image_line2` and
//! `av_write_image_line2` loops (with every `int` operation checked for UB
//! and every pointer offset tracked in i64) decides, for a random
//! descriptor, planes and line, whether C's accesses are defined and inside
//! the planes. The wrapper must accept exactly those; accepted lines run on
//! exactly-sized heap planes (ASan catches any byte past them).

use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::imgutils::{Planes, PlanesMut};
use ffmpeg::libavutil::pixdesc::{
    ComponentDescriptor, ImageLineError, PixelFormatDescriptor, PixelFormatDescriptorRef,
    PixelFormatFlags, read_image_line, read_image_line_u16, write_image_line,
    write_image_line_u16,
};
use ffmpeg::libavutil::pixfmt::PixelFormat;

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
    fn chance(&mut self, one_in: u64) -> bool {
        self.below(one_in) == 0
    }
}

/// The bytes C touches, as offsets from the row-0 pointer of one plane.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Foot {
    plane: usize,
    lo: i64,
    hi: i64,
    /// Bytes of plane 1 from its row-0 pointer a palette lookup needs
    /// (`max index + 1`), or a negative index.
    pal: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Sim {
    /// C returns before touching anything.
    Nothing,
    /// C's arithmetic is undefined (or a plane index outside `data[4]`).
    Ub(&'static str),
    Touch(Foot),
}

const BE: u32 = ffi::AV_PIX_FMT_FLAG_BE;
const BITSTREAM: u32 = ffi::AV_PIX_FMT_FLAG_BITSTREAM;

/// `av_read_image_line2` (`write == false`) or `av_write_image_line2` on a
/// descriptor whose slot `c` is `comp`, with the image's bytes all 0xFF
/// (so every value read, and every palette index, is its maximum).
fn simulate(
    comp: [i32; 5],
    flags: u32,
    (x, y, c, w): (i32, i32, i32, i32),
    linesizes: [isize; 4],
    write: bool,
    pal: bool,
) -> Sim {
    let [plane, step, offset, shift, depth] = comp;
    let pal = pal && !write;
    if w <= 0 {
        // The wrapper validates the positions and returns before C runs.
        return Sim::Nothing;
    }
    // Reader: `unsigned mask = (1ULL << depth) - 1;` before anything.
    let mut mask: u32 = 0;
    if !write {
        if !(0..64).contains(&depth) {
            return Sim::Ub("1ULL << depth");
        }
        mask = ((1u64 << depth) - 1) as u32;
    }
    if depth == 0 {
        return Sim::Nothing;
    }
    let Some(p) = usize::try_from(plane).ok().filter(|&p| p < 4) else {
        return Sim::Ub("data[plane]");
    };
    let Ok(ls) = i32::try_from(linesizes[p]) else {
        return Sim::Ub("linesize beyond int");
    };
    let Some(row) = y.checked_mul(ls) else {
        return Sim::Ub("y * linesize");
    };
    let row = i64::from(row);
    let (mut lo, mut hi) = (i64::MAX, i64::MIN);
    let mut touch = |a: i64, n: i64| {
        lo = lo.min(a);
        hi = hi.max(a + n);
    };
    // The palette index of the largest value read, as C computes it.
    let mut pal_need: Option<i64> = None;
    let mut pal_index = |val_unsigned: u32, int_arith: bool| -> Result<(), &'static str> {
        // data[1][4 * val + c]
        let idx: i64 = if int_arith {
            // `int val`: the unsigned value converted to int (wraps).
            let v = val_unsigned as i32;
            let four = v.checked_mul(4).ok_or("4 * val")?;
            i64::from(four.checked_add(c).ok_or("4 * val + c")?)
        } else {
            // `unsigned val`: 4 * val + c in unsigned arithmetic.
            i64::from(4u32.wrapping_mul(val_unsigned).wrapping_add(c as u32))
        };
        if idx < 0 {
            pal_need = Some(idx);
        } else {
            pal_need = Some(pal_need.map_or(idx + 1, |n| n.max(idx + 1)));
        }
        Ok(())
    };
    if flags & BITSTREAM != 0 {
        if step > 8 {
            if write {
                // uint32_t mask = ((1ULL << depth) - 1) << offset;
                if !(0..64).contains(&depth) {
                    return Sim::Ub("1ULL << depth (writer)");
                }
                if !(0..64).contains(&offset) {
                    return Sim::Ub("mask << offset");
                }
                // val << offset, val unsigned
                if !(0..32).contains(&offset) {
                    return Sim::Ub("val << offset");
                }
            } else {
                // val >> comp.offset, val int (negative is implementation-defined, not UB)
                if !(0..32).contains(&offset) {
                    return Sim::Ub("val >> offset");
                }
            }
            for i in 0..i64::from(w) {
                touch(row + 4 * i, 4);
                if pal {
                    // int val = AV_RB32(p) = 0xFFFFFFFF -> -1; -1 >> offset = -1; & mask = mask.
                    if let Err(e) = pal_index(mask, true) {
                        return Sim::Ub(e);
                    }
                }
            }
        } else {
            let Some(skip) = step.checked_mul(x).and_then(|s| s.checked_add(offset)) else {
                return Sim::Ub("skip");
            };
            let mut p = row + i64::from(skip >> 3);
            let Some(mut sh) = 8i32.checked_sub(depth).and_then(|s| s.checked_sub(skip & 7)) else {
                return Sim::Ub("8 - depth - (skip & 7)");
            };
            for _ in 0..w {
                // reader: (*p >> shift) & mask, *p promoted to int; writer: val << shift, val unsigned.
                if !(0..32).contains(&sh) {
                    return Sim::Ub("shift by *p / val");
                }
                touch(p, 1);
                if pal {
                    let val = ((255u32 >> sh) & mask) as u32;
                    if let Err(e) = pal_index(val, true) {
                        return Sim::Ub(e);
                    }
                }
                let Some(next) = sh.checked_sub(step) else {
                    return Sim::Ub("shift -= step");
                };
                sh = next;
                p -= i64::from(sh >> 3);
                sh &= 7;
            }
        }
    } else {
        let Some(xs) = x.checked_mul(step) else {
            return Sim::Ub("x * step");
        };
        let p0 = row + i64::from(xs) + i64::from(offset);
        let Some(sd) = shift.checked_add(depth) else {
            return Sim::Ub("shift + depth");
        };
        let (size, p0) = if sd <= 8 {
            (1i64, p0 + i64::from(flags & BE != 0))
        } else if sd <= 16 {
            (2, p0)
        } else {
            (4, p0)
        };
        // reader: (val >> shift) & mask, val unsigned; writer: s << shift, s unsigned.
        if !(0..32).contains(&shift) {
            return Sim::Ub("shift");
        }
        for i in 0..i64::from(w) {
            touch(p0 + i * i64::from(step), size);
            if pal {
                let readmax = ((1u64 << (8 * size)) - 1) as u32 >> shift;
                if let Err(e) = pal_index(readmax & mask, false) {
                    return Sim::Ub(e);
                }
            }
        }
    }
    Sim::Touch(Foot {
        plane: p,
        lo,
        hi,
        pal: pal_need,
    })
}

/// The wrapper's documented row-0 convention.
fn row0(len: usize, linesize: isize) -> usize {
    if linesize >= 0 || len == 0 {
        return 0;
    }
    let l = linesize.unsigned_abs();
    (len - 1) / l * l
}

/// Whether C's accesses lie inside the planes.
fn fits(f: &Foot, lens: [usize; 4], linesizes: [isize; 4]) -> bool {
    let at = row0(lens[f.plane], linesizes[f.plane]) as i64;
    if at + f.lo < 0 || at + f.hi > lens[f.plane] as i64 {
        return false;
    }
    if let Some(need) = f.pal {
        if need < 0 {
            return false;
        }
        let at1 = row0(lens[1], linesizes[1]) as i64;
        if lens[1] as i64 - at1 < need {
            return false;
        }
    }
    true
}

fn build(comp: [i32; 5], slot: usize, count: u8, flags: PixelFormatFlags) -> PixelFormatDescriptor {
    let mut d = PixelFormatDescriptor::new();
    let mut m = d.as_mut();
    m.set_component_count(count);
    m.set_flags(flags);
    let [plane, step, offset, shift, depth] = comp;
    m.components_mut()
        .get_mut(slot)
        .unwrap()
        .set(ComponentDescriptor::from_raw(ffi::AVComponentDescriptor {
            plane,
            step,
            offset,
            shift,
            depth,
        }));
    d
}

const INTS: [i32; 24] = [
    i32::MIN,
    i32::MIN + 1,
    -(1 << 30),
    -(1 << 28),
    -300,
    -40,
    -9,
    -8,
    -7,
    -3,
    -2,
    -1,
    0,
    1,
    2,
    3,
    4,
    7,
    8,
    9,
    16,
    31,
    32,
    33,
];
const BIG: [i32; 6] = [1 << 20, 1 << 27, (1 << 28) - 1, 1 << 28, 1 << 30, i32::MAX];

fn small_or_edge(rng: &mut Rng) -> i32 {
    match rng.below(10) {
        0 => rng.pick(&INTS),
        1 => rng.pick(&BIG),
        _ => rng.below(80) as i32 - 40,
    }
}

fn main() {
    let iters: u64 = std::env::args().nth(1).map_or(200_000, |s| s.parse().unwrap());
    let seed: u64 = std::env::args().nth(2).map_or(0x9e37_79b9_7f4a_7c15, |s| s.parse().unwrap());
    let mut rng = Rng(seed);
    let nb = ffi::AVPixelFormat::AV_PIX_FMT_NB.0;
    let table: Vec<PixelFormatDescriptorRef<'static>> = (0..nb)
        .filter_map(|r| PixelFormat::from_raw(ffi::AVPixelFormat(r)).descriptor())
        .filter(|d| d.name().is_some())
        .collect();
    let (mut accepted, mut refused, mut nothing, mut mismatches) = (0u64, 0u64, 0u64, 0u64);
    for it in 0..iters {
        // Descriptor: C's table, or one Rust-built component at the edges.
        let from_table = rng.chance(3);
        let c = rng.below(4) as usize;
        let built;
        let (desc, flags): (PixelFormatDescriptorRef<'_>, u32) = if from_table {
            let d = rng.pick(&table);
            (d, d.flags().bits() as u32)
        } else {
            let plane = match rng.below(8) {
                0 => rng.pick(&[-1, 4, 5, i32::MIN, i32::MAX]),
                _ => rng.below(4) as i32,
            };
            let step = match rng.below(4) {
                0 => rng.pick(&INTS),
                1 => rng.pick(&BIG),
                _ => rng.below(40) as i32 - 6,
            };
            let offset = small_or_edge(&mut rng);
            let shift = match rng.below(3) {
                0 => small_or_edge(&mut rng),
                _ => rng.below(34) as i32 - 1,
            };
            let depth = match rng.below(3) {
                0 => rng.pick(&[-1, 0, 1, 31, 32, 33, 63, 64, 65, i32::MIN, i32::MAX]),
                _ => rng.below(34) as i32 - 1,
            };
            let mut fl = PixelFormatFlags::empty();
            if rng.chance(2) {
                fl |= PixelFormatFlags::BITSTREAM;
            }
            if rng.chance(2) {
                fl |= PixelFormatFlags::BIG_ENDIAN;
            }
            if rng.chance(4) {
                fl |= PixelFormatFlags::PALETTE;
            }
            built = build([plane, step, offset, shift, depth], c, 4, fl);
            (built.as_ref(), fl.bits() as u32)
        };
        let comp = {
            let k = desc.components().get(c).unwrap();
            [k.plane(), k.step(), k.offset(), k.shift(), k.depth()]
        };
        // Planes.
        let lens: [usize; 4] = core::array::from_fn(|_| match rng.below(6) {
            0 => 0,
            1 => rng.below(8) as usize,
            2 => 1024 + rng.below(64) as usize,
            _ => rng.below(700) as usize,
        });
        let linesizes: [isize; 4] = core::array::from_fn(|_| match rng.below(12) {
            0 => rng.pick(&[isize::MIN, isize::MAX, i32::MIN as isize, i32::MAX as isize, (i32::MAX as isize) + 1, (i32::MIN as isize) - 1]),
            1 => 0,
            2 => rng.below(80) as isize - 80,
            _ => rng.below(160) as isize - 80,
        });
        let big_pos = |rng: &mut Rng| -> usize {
            match rng.below(16) {
                0 => rng.pick(&[1usize << 20, 1 << 30, i32::MAX as usize, (i32::MAX as usize) + 1, usize::MAX]),
                _ => rng.below(14) as usize,
            }
        };
        let (x, y) = (big_pos(&mut rng), big_pos(&mut rng));
        let w = match rng.below(16) {
            0 => rng.pick(&[0usize, 1 << 12]),
            _ => rng.below(12) as usize,
        };
        let write = rng.chance(2);
        let pal = !write && rng.chance(2);
        let what = format!(
            "it {it}: comp {comp:?} flags {flags:#x} c {c} lens {lens:?} ls {linesizes:?} (x {x}, y {y}, w {w}) write {write} pal {pal}"
        );
        // The simulation's verdict.
        let ints = (
            i32::try_from(x),
            i32::try_from(y),
            c as i32,
            i32::try_from(w),
        );
        let sim = match ints {
            (Ok(x), Ok(y), c, Ok(w)) => simulate(comp, flags, (x, y, c, w), linesizes, write, pal),
            _ => Sim::Ub("position beyond int"),
        };
        let expect_ok = match sim {
            Sim::Nothing => true,
            Sim::Ub(_) => false,
            Sim::Touch(f) => fits(&f, lens, linesizes),
        };
        // The wrapper, on exactly sized 0xFF planes.
        let mut img: [Vec<u8>; 4] = lens.map(|n| vec![0xFF; n]);
        let r: Result<(), ImageLineError> = if write {
            let src16: Vec<u16> = vec![u16::MAX; w];
            let src32: Vec<u32> = vec![u32::MAX; w];
            let mut planes = PlanesMut {
                data: img.each_mut().map(|p| &mut p[..]),
                linesizes,
            };
            let a = write_image_line(&src32, &mut planes, desc, x, y, c);
            let b = write_image_line(&src16, &mut planes, desc, x, y, c);
            let d = write_image_line_u16(&src16, &mut planes, desc, x, y, c);
            assert_eq!(a, b, "{what}");
            assert_eq!(a, d, "{what}");
            a
        } else {
            let planes = Planes {
                data: img.each_ref().map(|p| &p[..]),
                linesizes,
            };
            let mut d32 = vec![0u32; w];
            let mut d16 = vec![0u16; w];
            let mut d16b = vec![0u16; w];
            let a = read_image_line(&mut d32, planes, desc, x, y, c, pal);
            let b = read_image_line(&mut d16, planes, desc, x, y, c, pal);
            let d = read_image_line_u16(&mut d16b, planes, desc, x, y, c, pal);
            assert_eq!(a, b, "{what}");
            assert_eq!(a, d, "{what}");
            if a.is_ok() {
                assert_eq!(d16, d16b, "{what}");
                let trunc: Vec<u16> = d32.iter().map(|&v| v as u16).collect();
                assert_eq!(trunc, d16, "{what}");
            }
            a
        };
        match (r.is_ok(), expect_ok) {
            (true, true) => {
                if sim == Sim::Nothing {
                    nothing += 1;
                } else {
                    accepted += 1;
                }
            }
            (false, false) => refused += 1,
            (got, want) => {
                mismatches += 1;
                eprintln!("MISMATCH wrapper ok={got} sim ok={want}: {what}\n  wrapper: {r:?}\n  sim: {sim:?}");
                if mismatches > 20 {
                    std::process::exit(1);
                }
            }
        }
    }
    eprintln!("accepted {accepted} nothing {nothing} refused {refused} mismatches {mismatches}");
    if mismatches > 0 {
        std::process::exit(1);
    }
}
