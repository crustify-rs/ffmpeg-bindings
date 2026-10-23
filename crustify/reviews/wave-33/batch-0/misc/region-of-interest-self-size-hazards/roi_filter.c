/* Feeds one frame whose REGIONS_OF_INTEREST side data holds one 28-byte
 * AVRegionOfInterest with the given self_size through `filter`
 * (buffer -> filter -> buffersink).
 * Usage: roi_filter <self_size> [filter [filter args]] (default: showinfo). */
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include "libavfilter/avfilter.h"
#include "libavfilter/buffersink.h"
#include "libavfilter/buffersrc.h"
#include "libavutil/frame.h"

int main(int argc, char **argv)
{
    uint32_t self_size = argc > 1 ? (uint32_t)strtoul(argv[1], NULL, 0) : 4;
    const char *name = argc > 2 ? argv[2] : "showinfo";
    const char *args = argc > 3 ? argv[3] : NULL;
    AVFilterGraph *g = avfilter_graph_alloc();
    AVFilterContext *src, *mid, *sink;
    if (avfilter_graph_create_filter(&src, avfilter_get_by_name("buffer"), "in",
            "video_size=16x16:pix_fmt=yuv420p:time_base=1/25", NULL, g) < 0 ||
        avfilter_graph_create_filter(&mid, avfilter_get_by_name(name), "mid",
            args, NULL, g) < 0 ||
        avfilter_graph_create_filter(&sink, avfilter_get_by_name("buffersink"),
            "out", NULL, NULL, g) < 0 ||
        avfilter_link(src, 0, mid, 0) < 0 || avfilter_link(mid, 0, sink, 0) < 0 ||
        avfilter_graph_config(g, NULL) < 0)
        return 2;
    AVFrame *f = av_frame_alloc();
    f->width = f->height = 16;
    f->format = AV_PIX_FMT_YUV420P;
    if (av_frame_get_buffer(f, 0) < 0)
        return 2;
    AVFrameSideData *sd = av_frame_new_side_data(f, AV_FRAME_DATA_REGIONS_OF_INTEREST,
                                                 sizeof(AVRegionOfInterest));
    if (!sd)
        return 2;
    AVRegionOfInterest roi = { .self_size = self_size, .top = 0, .bottom = 8,
                               .left = 0, .right = 8, .qoffset = { -1, 10 } };
    memcpy(sd->data, &roi, sizeof(roi));
    printf("payload %zu bytes, self_size %u\n", sd->size, self_size);
    fflush(stdout);
    if (av_buffersrc_add_frame(src, f) < 0)
        return 2;
    AVFrame *out = av_frame_alloc();
    int ret = av_buffersink_get_frame(sink, out);
    printf("filtered: %d\n", ret);
    av_frame_free(&out);
    av_frame_free(&f);
    avfilter_graph_free(&g);
    return 0;
}
