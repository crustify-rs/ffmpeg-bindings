// The class of a message's context is borrowed for the callback only.
use std::cell::RefCell;

use ffmpeg::libavutil::log::{Callback, ClassRef, Record};

thread_local!(static KEPT: RefCell<Option<ClassRef<'static>>> = const { RefCell::new(None) });

struct Keep;

impl Callback for Keep {
    fn log(record: Record<'_>) {
        KEPT.with(|k| *k.borrow_mut() = record.class());
    }
}

fn main() {}
