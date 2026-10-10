/* crustify_<NAME> shims for libavutil function-like macros bindgen cannot bind. */
#ifndef CRUSTIFY_LIBAVUTIL_SHIM_H
#define CRUSTIFY_LIBAVUTIL_SHIM_H

#include <stddef.h>
#include <stdint.h>

#include "libavutil/bprint.h"
#include "libavutil/log.h"
#include "libavutil/pixfmt.h"

/* av_vlog with its va_list built from the trailing arguments: stable Rust
 * can call a C variadic function but cannot construct a va_list. */
void crustify_av_vlog(void *avcl, int level, const char *fmt, ...);

/* av_vbprintf with its va_list built from the trailing arguments, as for
 * crustify_av_vlog. */
void crustify_av_vbprintf(AVBPrint *buf, const char *fmt, ...);

/* libavutil/intreadwrite.h's aligned accessors, which expand to member
 * accesses on a pointer cast to av_alias64: the C references for the
 * Alias64 wrapper's equivalence tests. Every pointer must be 8-byte aligned,
 * as the macros require. */
uint64_t crustify_AV_RN64A(const void *p);
void crustify_AV_WN64A(void *p, uint64_t v);
void crustify_AV_COPY64(void *d, const void *s);
void crustify_AV_SWAP64(void *a, void *b);
void crustify_AV_ZERO64(void *d);

/* AV_IS_INPUT_DEVICE / AV_IS_OUTPUT_DEVICE: whether a class category is one
 * of the three input (output) device categories; 1 or 0. */
int crustify_AV_IS_INPUT_DEVICE(AVClassCategory category);
int crustify_AV_IS_OUTPUT_DEVICE(AVClassCategory category);

/* The same accessors over av_alias16 and av_alias32 (AV_SWAP has no 16- or
 * 32-bit form): the C references for the Alias16 / Alias32 equivalence tests.
 * Every pointer must be 2- or 4-byte aligned, as the macros require. */
uint16_t crustify_AV_RN16A(const void *p);
void crustify_AV_WN16A(void *p, uint16_t v);
void crustify_AV_COPY16(void *d, const void *s);
void crustify_AV_ZERO16(void *d);
uint32_t crustify_AV_RN32A(const void *p);
void crustify_AV_WN32A(void *p, uint32_t v);
void crustify_AV_COPY32(void *d, const void *s);
void crustify_AV_ZERO32(void *d);

/* The native-endian AV_PIX_FMT_<name> aliases of libavutil/pixfmt.h, which
 * paste tokens through AV_PIX_FMT_NE and so escape bindgen: the C references
 * for PixelFormat's native-endian constants in equivalence tests. Returns the
 * i-th alias and stores its <name> in *name, or returns AV_PIX_FMT_NONE and
 * stores NULL once i is past the last one. */
enum AVPixelFormat crustify_AV_PIX_FMT_NE(size_t i, const char **name);

#endif /* CRUSTIFY_LIBAVUTIL_SHIM_H */
