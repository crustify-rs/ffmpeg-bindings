/* Pure-C probe: reads through glibc strto* end pointers after earlier
 * parses freed blocks, at a caller-chosen stack depth. No Rust involved. */
#include <alloca.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "libavutil/eval.h"
#include "libavutil/parseutils.h"

static int run(const char *which)
{
    double d;
    int w, h;
    uint8_t rgba[4];
    if (!strcmp(which, "size"))
        return av_parse_video_size(&w, &h, "640x480");   /* parseutils.c:166 */
    if (!strcmp(which, "neg"))
        return av_expr_parse_and_eval(&d, "-3", NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL); /* eval.c:581 */
    return av_parse_color(rgba, "0xff00ff", -1, NULL);   /* parseutils.c:391 */
}

int main(int argc, char **argv)
{
    int pad = atoi(argv[2]);
    double d;
    uint8_t rgba[4];
    /* Parses that allocate and free, leaving freed pointers in dead frames. */
    av_expr_parse_and_eval(&d, argv[3], NULL, NULL, NULL, NULL, NULL, NULL, NULL, 0, NULL);
    av_parse_color(rgba, "red@0.5", -1, NULL);
    volatile char *p = alloca(pad + 1);
    p[0] = 0;
    return run(argv[1]) < 0;
}
