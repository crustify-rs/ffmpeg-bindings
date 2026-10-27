#include "wrapper.h"

// Static wrappers

char * av_make_error_string__extern(char *errbuf, size_t errbuf_size, int errnum) { return av_make_error_string(errbuf, errbuf_size, errnum); }
int av_clip_c__extern(int a, int amin, int amax) { return av_clip_c(a, amin, amax); }
int64_t av_clip64_c__extern(int64_t a, int64_t amin, int64_t amax) { return av_clip64_c(a, amin, amax); }
uint8_t av_clip_uint8_c__extern(int a) { return av_clip_uint8_c(a); }
int8_t av_clip_int8_c__extern(int a) { return av_clip_int8_c(a); }
uint16_t av_clip_uint16_c__extern(int a) { return av_clip_uint16_c(a); }
int16_t av_clip_int16_c__extern(int a) { return av_clip_int16_c(a); }
int32_t av_clipl_int32_c__extern(int64_t a) { return av_clipl_int32_c(a); }
int av_clip_intp2_c__extern(int a, int p) { return av_clip_intp2_c(a, p); }
unsigned int av_clip_uintp2_c__extern(int a, int p) { return av_clip_uintp2_c(a, p); }
unsigned int av_zero_extend_c__extern(unsigned int a, unsigned int p) { return av_zero_extend_c(a, p); }
int av_sat_add32_c__extern(int a, int b) { return av_sat_add32_c(a, b); }
int av_sat_dadd32_c__extern(int a, int b) { return av_sat_dadd32_c(a, b); }
int av_sat_sub32_c__extern(int a, int b) { return av_sat_sub32_c(a, b); }
int av_sat_dsub32_c__extern(int a, int b) { return av_sat_dsub32_c(a, b); }
int64_t av_sat_add64_c__extern(int64_t a, int64_t b) { return av_sat_add64_c(a, b); }
int64_t av_sat_sub64_c__extern(int64_t a, int64_t b) { return av_sat_sub64_c(a, b); }
float av_clipf_c__extern(float a, float amin, float amax) { return av_clipf_c(a, amin, amax); }
double av_clipd_c__extern(double a, double amin, double amax) { return av_clipd_c(a, amin, amax); }
int av_ceil_log2_c__extern(int x) { return av_ceil_log2_c(x); }
int av_popcount_c__extern(uint32_t x) { return av_popcount_c(x); }
int av_popcount64_c__extern(uint64_t x) { return av_popcount64_c(x); }
int av_parity_c__extern(uint32_t v) { return av_parity_c(v); }
AVRational av_make_q__extern(int num, int den) { return av_make_q(num, den); }
int av_cmp_q__extern(AVRational a, AVRational b) { return av_cmp_q(a, b); }
double av_q2d__extern(AVRational a) { return av_q2d(a); }
AVRational av_inv_q__extern(AVRational q) { return av_inv_q(q); }
float av_int2float__extern(uint32_t i) { return av_int2float(i); }
uint32_t av_float2int__extern(float f) { return av_float2int(f); }
double av_int2double__extern(uint64_t i) { return av_int2double(i); }
uint64_t av_double2int__extern(double f) { return av_double2int(f); }
void * av_x_if_null__extern(const void *p, const void *x) { return av_x_if_null(p, x); }
const AVFrameSideData * av_frame_side_data_get__extern(AVFrameSideData *const *sd, const int nb_sd, enum AVFrameSideDataType type) { return av_frame_side_data_get(sd, nb_sd, type); }
size_t av_strnlen__extern(const char *s, size_t len) { return av_strnlen(s, len); }
int av_isdigit__extern(int c) { return av_isdigit(c); }
int av_isgraph__extern(int c) { return av_isgraph(c); }
int av_isspace__extern(int c) { return av_isspace(c); }
int av_toupper__extern(int c) { return av_toupper(c); }
int av_tolower__extern(int c) { return av_tolower(c); }
int av_isxdigit__extern(int c) { return av_isxdigit(c); }
int av_bprint_is_complete__extern(const AVBPrint *buf) { return av_bprint_is_complete(buf); }
uint16_t av_bswap16__extern(uint16_t x) { return av_bswap16(x); }
uint32_t av_bswap32__extern(uint32_t x) { return av_bswap32(x); }
uint64_t av_bswap64__extern(uint64_t x) { return av_bswap64(x); }
void av_image_copy2__extern(uint8_t *const dst_data [4], const int dst_linesizes [4], uint8_t *const src_data [4], const int src_linesizes [4], enum AVPixelFormat pix_fmt, int width, int height) { av_image_copy2(dst_data, dst_linesizes, src_data, src_linesizes, pix_fmt, width, height); }
void * av_refstruct_alloc_ext__extern(size_t size, unsigned int flags, void *opaque, void (*free_cb) (AVRefStructOpaque opaque, void *obj)) { return av_refstruct_alloc_ext(size, flags, opaque, free_cb); }
void * av_refstruct_allocz__extern(size_t size) { return av_refstruct_allocz(size); }
AVRefStructPool * av_refstruct_pool_alloc_ext__extern(size_t size, unsigned int flags, void *opaque, int (*init_cb) (AVRefStructOpaque opaque, void *obj), void (*reset_cb) (AVRefStructOpaque opaque, void *obj), void (*free_entry_cb) (AVRefStructOpaque opaque, void *obj), void (*free_cb) (AVRefStructOpaque opaque)) { return av_refstruct_pool_alloc_ext(size, flags, opaque, init_cb, reset_cb, free_entry_cb, free_cb); }
void av_refstruct_pool_uninit__extern(AVRefStructPool **poolp) { av_refstruct_pool_uninit(poolp); }
char * av_ts_make_string__extern(char *buf, int64_t ts) { return av_ts_make_string(buf, ts); }
char * av_ts_make_time_string__extern(char *buf, int64_t ts, const AVRational *tb) { return av_ts_make_time_string(buf, ts, tb); }
int av_uuid_equal__extern(const AVUUID uu1, const AVUUID uu2) { return av_uuid_equal(uu1, uu2); }
void av_uuid_copy__extern(AVUUID dest, const AVUUID src) { av_uuid_copy(dest, src); }
void av_uuid_nil__extern(AVUUID uu) { av_uuid_nil(uu); }
