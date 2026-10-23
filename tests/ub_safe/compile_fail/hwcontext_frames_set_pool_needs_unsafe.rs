use ffmpeg_bindings::libavutil::libavutil::buffer::BufferPool;
use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitFrames;

fn bytes_as_surfaces(mut frames: UninitFrames) {
    let pool = BufferPool::new(64).unwrap();
    let _ = frames.context_mut().set_pool(pool);
}

fn main() {}
