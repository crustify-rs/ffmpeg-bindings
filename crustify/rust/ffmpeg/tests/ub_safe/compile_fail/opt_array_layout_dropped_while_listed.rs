// The layouts an array edit copies are borrowed: a layout cannot drop
// while a handle to it is listed among the values.
use ffmpeg::libavutil::channel_layout::ChannelLayout;
use ffmpeg::libavutil::log::{Class, ClassCategory};
use ffmpeg::libavutil::opt::{BareObject, SearchFlags};

static CLASS: Class = Class::new(c"c", ClassCategory::NA);

fn main() {
    let mut o = BareObject::new(CLASS.as_ref()).unwrap();
    let layout = ChannelLayout::parse(c"stereo").unwrap();
    let refs = [layout.as_ref()];
    drop(layout);
    let _ = o.as_object_mut().insert_array(c"x", 0, &refs, SearchFlags::NONE);
}
