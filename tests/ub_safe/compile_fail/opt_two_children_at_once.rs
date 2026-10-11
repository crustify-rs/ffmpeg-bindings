// The child cursor lends one child at a time: the next child cannot be taken
// while the previous one is in use.
use ffmpeg::libavutil::log::{Class, ClassCategory};
use ffmpeg::libavutil::opt::BareObject;

static CLASS: Class = Class::new(c"parent", ClassCategory::NA);

fn main() {
    let mut obj = BareObject::new(CLASS.as_ref()).unwrap();
    let mut obj = obj.as_object_mut();
    let mut cursor = obj.children_mut();
    let first = cursor.next_child();
    let second = cursor.next_child();
    drop((first, second));
}
