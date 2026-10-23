// A child handle borrows its parent object: it cannot outlive it.
use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory};
use ffmpeg_bindings::libavutil::libavutil::opt::BareObject;

static CLASS: Class = Class::new(c"parent", ClassCategory::NA);

fn main() {
    let child;
    {
        let parent = BareObject::new(CLASS.as_ref()).unwrap();
        child = parent.as_object().children().next();
    }
    println!("{child:?}");
}
