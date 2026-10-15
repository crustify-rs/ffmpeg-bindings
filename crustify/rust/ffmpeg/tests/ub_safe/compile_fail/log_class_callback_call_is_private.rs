// A class's callbacks run on an object through `ObjectRef::item_name` and
// `ObjectRef::category`: running one class's callback on an arbitrary
// pointer is not public.
use ffmpeg::libavutil::log::{Class, ClassCategory};

static CLASS: Class = Class::new(c"c", ClassCategory::NA);

fn main() {
    if let Some(f) = CLASS.as_ref().item_name() {
        let _ = unsafe { f.call(core::ptr::null_mut()) };
    }
}
