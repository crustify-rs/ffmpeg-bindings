/* crustify_<NAME> shims for libavutil function-like macros bindgen cannot bind. */
#ifndef CRUSTIFY_LIBAVUTIL_SHIM_H
#define CRUSTIFY_LIBAVUTIL_SHIM_H

/* av_vlog with its va_list built from the trailing arguments: stable Rust
 * can call a C variadic function but cannot construct a va_list. */
void crustify_av_vlog(void *avcl, int level, const char *fmt, ...);

#endif /* CRUSTIFY_LIBAVUTIL_SHIM_H */
