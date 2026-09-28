/* A thin shim over jbigkit (JBIG, ITU-T T.82), so the Rust side never
 * lays out jbigkit's structs itself: one call to encode an image, one to
 * decode one. Images are rows of packed bits, most significant bit
 * first, 1 a set (black) pixel -- jbigkit's own layout. */

#include <jbig.h>
#include <string.h>

/* Where the encoder's output goes: a buffer the caller owns. */
struct out_buffer {
    unsigned char *data; /* the buffer */
    size_t length;       /* bytes written so far */
    size_t capacity;     /* bytes it holds */
    int overflowed;      /* whether the output did not fit */
};

/* jbigkit's output callback: appends to the buffer, or notes that it
 * did not fit. */
static void collect(unsigned char *start, size_t length, void *file) {
    struct out_buffer *out = file;
    if (out->length + length > out->capacity) {
        out->overflowed = 1;
        return;
    }
    memcpy(out->data + out->length, start, length);
    out->length += length;
}

/* Encodes `rows` (`width` by `height`) into `out`, as one stripe with
 * jbigkit's default options; returns the bytes written, or 0 if they
 * did not fit in `capacity`. */
size_t comparison_jbig_encode(unsigned char *rows, unsigned long width, unsigned long height,
                              unsigned char *out, size_t capacity) {
    struct jbg_enc_state state;
    unsigned char *planes[1] = {rows};
    struct out_buffer buffer = {out, 0, capacity, 0};
    jbg_enc_init(&state, width, height, 1, planes, collect, &buffer);
    jbg_enc_options(&state, -1, -1, height, -1, -1);
    jbg_enc_out(&state);
    jbg_enc_free(&state);
    return buffer.overflowed ? 0 : buffer.length;
}

/* Decodes `length` bytes of `in` into `rows`, `rows_length` bytes;
 * returns 0 on success, else nonzero. */
int comparison_jbig_decode(unsigned char *in, size_t length, unsigned char *rows, size_t rows_length) {
    struct jbg_dec_state state;
    size_t read;
    jbg_dec_init(&state);
    int result = jbg_dec_in(&state, in, length, &read);
    if (result != JBG_EOK || jbg_dec_getsize(&state) != rows_length) {
        jbg_dec_free(&state);
        return result == JBG_EOK ? -1 : result;
    }
    memcpy(rows, jbg_dec_getimage(&state, 0), rows_length);
    jbg_dec_free(&state);
    return 0;
}
