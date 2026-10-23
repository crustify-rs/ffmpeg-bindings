// The object a device context's handle converts into borrows the context:
// it cannot outlive the device that owns it.
use ffmpeg_bindings::libavutil::libavutil::hwcontext::UninitDevice;
use ffmpeg_bindings::libavutil::libavutil::opt::ObjectRef;

fn object(dev: UninitDevice) -> ObjectRef<'static> {
    ObjectRef::from(dev.context())
}

fn main() {}
