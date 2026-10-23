// A handle to a Rust-built class borrows it.
use ffmpeg_bindings::libavutil::libavutil::log::{Class, ClassCategory, ClassRef};

fn handle() -> ClassRef<'static> {
    let class = Class::new(c"local", ClassCategory::NA);
    class.as_ref()
}

fn main() {
    println!("{:?}", handle().name());
}
