// Forcing the CPU flags is `unsafe`: safe code cannot enable an unsupported
// extension.
fn main() {
    ffmpeg_bindings::libavutil::libavutil::cpu::force_cpu_flags(-1);
}
