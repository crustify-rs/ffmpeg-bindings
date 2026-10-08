/* crustify_<NAME> shims for libavutil function-like macros bindgen cannot bind. */
#ifndef CRUSTIFY_LIBAVUTIL_SHIM_H
#define CRUSTIFY_LIBAVUTIL_SHIM_H

#include <stdint.h>

#include "libavutil/log.h"

/* av_vlog with its va_list built from the trailing arguments: stable Rust
 * can call a C variadic function but cannot construct a va_list. */
void crustify_av_vlog(void *avcl, int level, const char *fmt, ...);

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

#endif /* CRUSTIFY_LIBAVUTIL_SHIM_H */
