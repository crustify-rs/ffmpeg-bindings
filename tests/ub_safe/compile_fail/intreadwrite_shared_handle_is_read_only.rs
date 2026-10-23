// The shared handle has getters only: writing through it is rejected.
use ffmpeg_bindings::libavutil::libavutil::intreadwrite::Alias32;

fn main() {
    let a = Alias32::from_u32(1);
    a.as_ref().set_u32(2);
}
