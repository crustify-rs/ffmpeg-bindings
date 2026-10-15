// The object a device context's handle converts into borrows the context:
// it cannot outlive the device that owns it.
use ffmpeg::libavutil::hwcontext::UninitDevice;
use ffmpeg::libavutil::opt::ObjectRef;

fn object(dev: UninitDevice) -> ObjectRef<'static> {
    ObjectRef::from(dev.context())
}

fn main() {}
