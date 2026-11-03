// The context a message was logged in is borrowed for the callback only.
use std::cell::RefCell;

use ffmpeg_bindings::libavutil::libavutil::log::{Callback, LogContext, Record};

thread_local!(static KEPT: RefCell<Option<LogContext<'static>>> = const { RefCell::new(None) });

struct Keep;

impl Callback for Keep {
    fn log(record: Record<'_>) {
        KEPT.with(|k| *k.borrow_mut() = record.context());
    }
}

fn main() {}
