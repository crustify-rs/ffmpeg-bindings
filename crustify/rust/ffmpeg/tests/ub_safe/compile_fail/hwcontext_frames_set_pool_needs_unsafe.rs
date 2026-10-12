use ffmpeg::libavutil::buffer::BufferPool;
use ffmpeg::libavutil::hwcontext::UninitFrames;

fn bytes_as_surfaces(mut frames: UninitFrames) {
    let pool = BufferPool::new(64).unwrap();
    let _ = frames.context_mut().set_pool(pool);
}

fn main() {}
