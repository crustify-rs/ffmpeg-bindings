// A handle borrows its tree from the box, which cannot be dropped first.
use ffmpeg_bindings::libavutil::libavutil::eval::Expr;

fn main() {
    let e = Expr::parse(c"1", &[], &[], &[], 0).unwrap();
    let r = e.as_ref();
    drop(e);
    let _ = r;
}
