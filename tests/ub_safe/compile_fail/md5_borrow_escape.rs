use ffmpeg_bindings::libavutil::libavutil::md5::{Md5, Md5Ref};
fn escaped<'a>() -> Md5Ref<'a> {
    let owner = Md5::new().unwrap();
    owner.as_ref()
}
fn main() {}
