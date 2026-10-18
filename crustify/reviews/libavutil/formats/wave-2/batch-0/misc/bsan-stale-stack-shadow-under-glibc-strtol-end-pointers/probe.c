#include <stdio.h>
#include <string.h>
#include <libavutil/channel_layout.h>
int main(int argc, char **argv) {
    for (int i = 1; i < argc; i++) {
        if (!strncmp(argv[i], "ch:", 3)) {
            enum AVChannel c = av_channel_from_string(argv[i] + 3);
            fprintf(stderr, "%s -> %d\n", argv[i], (int)c);
            continue;
        }
        AVChannelLayout l = {0};
        int r = av_channel_layout_from_string(&l, argv[i]);
        fprintf(stderr, "%s -> %d (%d ch, order %d)\n", argv[i], r, l.nb_channels, (int)l.order);
        av_channel_layout_uninit(&l);
    }
    return 0;
}
