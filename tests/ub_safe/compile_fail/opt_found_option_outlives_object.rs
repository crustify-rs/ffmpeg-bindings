//! An option found through an object's handle borrows the object's class
//! for the object's borrow, and cannot outlive the object.
use ffmpeg::libavutil::log::{Class, ClassCategory};
use ffmpeg::libavutil::opt::{BareObject, SearchFlags};

fn main() {
    let found = {
        let class = Class::new(c"local", ClassCategory::NA);
        let bare = BareObject::new(class.as_ref()).unwrap();
        bare.as_object().find(c"x", None, 0, SearchFlags::NONE)
    };
    assert!(found.is_none());
}
