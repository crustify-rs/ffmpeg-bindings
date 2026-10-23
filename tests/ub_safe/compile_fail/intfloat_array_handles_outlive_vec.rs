// A run of handles over a `HeapVec<IntFloat64>` cannot outlive the array
// that frees its block.
use ffmpeg_bindings::libavutil::libavutil::intfloat_h::IntFloat64;
use ffmpeg_bindings::libavutil::libavutil::mem::alloc_cell_array_zeroed;

fn main() {
    let run = {
        let a = alloc_cell_array_zeroed::<IntFloat64>(2).unwrap();
        a.as_handles()
    };
    println!("{}", run.get(0).unwrap().bits());
}
