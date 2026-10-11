// A function an expression calls borrows what it captures: the table cannot
// outlive the captured state.
use ffmpeg::libavutil::eval::{Func1, parse_and_eval};

fn main() {
    let funcs: [Func1<'_>; 1];
    {
        let scale = 2.0;
        let f = |x: f64| x * scale;
        funcs = [(c"f", &f)];
    }
    let _ = parse_and_eval(c"f(1)", &[], &funcs, &[], 0, None);
}
