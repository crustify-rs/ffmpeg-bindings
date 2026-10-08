// A string default borrows from the option's table like its name.
use ffmpeg::libavutil::opt::{OptionDefRef, OptionDefault};

fn default<'a>(o: OptionDefRef<'a>) -> OptionDefault<'static> {
    o.default_value()
}

fn main() {}
