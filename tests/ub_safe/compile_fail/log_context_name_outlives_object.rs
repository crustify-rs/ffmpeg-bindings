// A logging context's name is borrowed from the object it names.
use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory, LogContext};
use ffmpeg_bindings::libavutil::libavutil::opt::BareObject;

static CLASS: Class = Class::new(c"c", ClassCategory::NA);

fn main() {
    let name = {
        let obj = BareObject::new(CLASS.as_ref()).unwrap();
        LogContext::from(obj.as_object()).item_name()
    };
    drop(name);
}
