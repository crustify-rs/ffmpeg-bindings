// An exclusive object handle excludes every other handle while it lives.
use ffmpeg::libavutil::log::{Class, ClassCategory};
use ffmpeg::libavutil::opt::BareObject;

static CLASS: Class = Class::new(c"object", ClassCategory::NA);

fn main() {
    let mut obj = BareObject::new(CLASS.as_ref()).unwrap();
    let mut exclusive = obj.as_object_mut();
    let shared = obj.as_object();
    exclusive.set_defaults();
    println!("{shared:?}");
}
