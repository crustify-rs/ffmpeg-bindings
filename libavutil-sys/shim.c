#include <stdarg.h>

#include "libavutil/intreadwrite.h"
#include "libavutil/log.h"
#include "libavutil/macros.h"

#include "shim.h"

void crustify_av_vlog(void *avcl, int level, const char *fmt, ...)
{
    va_list vl;
    va_start(vl, fmt);
    av_vlog(avcl, level, fmt, vl);
    va_end(vl);
}

uint64_t crustify_AV_RN64A(const void *p)
{
    return AV_RN64A(p);
}

void crustify_AV_WN64A(void *p, uint64_t v)
{
    AV_WN64A(p, v);
}

void crustify_AV_COPY64(void *d, const void *s)
{
    AV_COPY64(d, s);
}

void crustify_AV_SWAP64(void *a, void *b)
{
    AV_SWAP64(a, b);
}

void crustify_AV_ZERO64(void *d)
{
    AV_ZERO64(d);
}

int crustify_AV_IS_INPUT_DEVICE(AVClassCategory category)
{
    return AV_IS_INPUT_DEVICE(category);
}

int crustify_AV_IS_OUTPUT_DEVICE(AVClassCategory category)
{
    return AV_IS_OUTPUT_DEVICE(category);
}
