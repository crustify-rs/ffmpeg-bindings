#include <stdarg.h>

#include "libavutil/log.h"

#include "shim.h"

void crustify_av_vlog(void *avcl, int level, const char *fmt, ...)
{
    va_list vl;
    va_start(vl, fmt);
    av_vlog(avcl, level, fmt, vl);
    va_end(vl);
}
