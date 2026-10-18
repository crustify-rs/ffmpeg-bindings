//! `Rational::nearest` against `av_find_nearest_q_idx` on lists holding two
//! candidates with an `INT_MIN` denominator that C never compares with each
//! other. C finishes (every comparison pairs one of them with a candidate
//! whose denominator is not `INT_MIN`, which `av_nearer_q` handles); the
//! wrapper refused every such list with `None`.
//!
//! Exits 1 on the affected commit; 0 once the wrapper walks C's loop.
use ffmpeg::libavutil::ffi;
use ffmpeg::libavutil::rational::Rational;

fn main() {
    let m = (1, i32::MIN);
    let cases: [((i32, i32), &[(i32, i32)]); 3] = [
        ((1, 3), &[(1, 3), m, (2, 3), m]),
        ((1, 2), &[(1, 3), m, m]),
        ((1, 3), &[(2, 3), m, (1, 3), m]),
    ];
    let mut differ = 0;
    for (q, list) in cases {
        let rust: Vec<Rational> = list.iter().map(|&(n, d)| Rational::new(n, d)).collect();
        let mut c: Vec<ffi::AVRational> = list.iter().map(|&(num, den)| ffi::AVRational { num, den }).collect();
        c.push(ffi::AVRational { num: 0, den: 0 });
        // SAFETY: `c` ends in `{0, 0}`, which C reads up to and never writes.
        // Every comparison C makes on these lists pairs at most one `INT_MIN`
        // denominator: `q`'s exact match, or `1/3`, stays the nearest.
        let want = unsafe { ffi::av_find_nearest_q_idx(ffi::AVRational { num: q.0, den: q.1 }, c.as_ptr()) };
        let got = Rational::new(q.0, q.1).nearest(&rust);
        let verdict = if got == Some(want as usize) { "agree" } else { "DIFFER" };
        println!("{q:?} in {list:?}: C {want}, Rust {got:?}: {verdict}");
        differ += (verdict == "DIFFER") as i32;
    }
    std::process::exit((differ > 0) as i32);
}
