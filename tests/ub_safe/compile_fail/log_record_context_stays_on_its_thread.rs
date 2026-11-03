// The context a message was logged in stays on the logging thread: whether
// the object may move between threads is its concrete type's to say.
use ffmpeg_bindings::libavutil::libavutil::log::{Callback, Record};

fn sent<T: Send>(_: T) {}

struct Send_;

impl Callback for Send_ {
    fn log(record: Record<'_>) {
        sent(record.context());
    }
}

fn main() {}
