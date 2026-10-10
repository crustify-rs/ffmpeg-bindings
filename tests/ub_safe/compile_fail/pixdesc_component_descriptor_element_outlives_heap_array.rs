// An element handle of an `av_malloc`-owned array cannot outlive the array.
use ffmpeg::libavutil::mem::alloc_cell_array_zeroed;
use ffmpeg::libavutil::pixdesc::ComponentDescriptor;

fn main() {
    let comps = alloc_cell_array_zeroed::<ComponentDescriptor>(4).unwrap();
    let first = comps.as_handles().get(0).unwrap();
    drop(comps);
    println!("{}", first.plane());
}
