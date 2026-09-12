// Freestanding C runtime widget for xtop (wasm32, no libc, no WASI).
//
// Exports the xtop guest ABI (see docs/wasm-widgets.md):
//   alloc, dealloc, manifest, render, result_len
//
// It parses the few State fields it needs straight out of the JSON with a
// tiny scanner (no JSON library) and answers with a draw list. The host only
// links `host.log`, so any libc/WASI import would fail instantiation — this
// module is deliberately freestanding.
//
// Build: ./build.sh

typedef unsigned int u32;
typedef unsigned long long u64;
typedef int i32;

// Bump allocator for the state payload (the host allocs once per tick and
// frees right after, so resetting on dealloc is enough).
static unsigned char heap[65536];
static u32 heap_used = 0;

// Draw-list output buffer.
static char out[8192];
static u32 out_len = 0;

void *memcpy(void *dst, const void *src, u32 n) {
    unsigned char *d = (unsigned char *)dst;
    const unsigned char *s = (const unsigned char *)src;
    for (u32 i = 0; i < n; i++) {
        d[i] = s[i];
    }
    return dst;
}

void *memset(void *dst, int value, u32 n) {
    unsigned char *d = (unsigned char *)dst;
    for (u32 i = 0; i < n; i++) {
        d[i] = (unsigned char)value;
    }
    return dst;
}

static u32 slen(const char *s) {
    u32 n = 0;
    while (s[n]) {
        n++;
    }
    return n;
}

static u32 put(char *dst, u32 pos, const char *src, u32 n) {
    for (u32 i = 0; i < n; i++) {
        dst[pos + i] = src[i];
    }
    return pos + n;
}

static u32 put_str(char *dst, u32 pos, const char *src) {
    return put(dst, pos, src, slen(src));
}

static u32 put_u64(char *dst, u32 pos, u64 value) {
    char tmp[20];
    u32 n = 0;
    if (value == 0) {
        dst[pos++] = '0';
        return pos;
    }
    while (value > 0) {
        tmp[n++] = (char)('0' + (value % 10));
        value /= 10;
    }
    while (n > 0) {
        dst[pos++] = tmp[--n];
    }
    return pos;
}

// First unsigned integer following `key` in `json` (0 when absent).
static u64 find_u64(const char *json, u32 len, const char *key) {
    u32 key_len = slen(key);
    for (u32 i = 0; i + key_len <= len; i++) {
        u32 k = 0;
        while (k < key_len && json[i + k] == key[k]) {
            k++;
        }
        if (k != key_len) {
            continue;
        }
        u32 j = i + key_len;
        while (j < len && (json[j] == ' ' || json[j] == '\t')) {
            j++;
        }
        u64 value = 0;
        int digits = 0;
        while (j < len && json[j] >= '0' && json[j] <= '9') {
            value = value * 10 + (u64)(json[j] - '0');
            j++;
            digits = 1;
        }
        if (digits) {
            return value;
        }
    }
    return 0;
}

__attribute__((export_name("alloc"))) i32 alloc(i32 len) {
    if (len <= 0 || heap_used + (u32)len > (u32)sizeof(heap)) {
        return 0;
    }
    u32 ptr = (u32)(unsigned long)(void *)&heap[heap_used];
    heap_used += (u32)len;
    return (i32)ptr;
}

__attribute__((export_name("dealloc"))) void dealloc(i32 ptr, i32 len) {
    (void)ptr;
    (void)len;
    heap_used = 0;
}

__attribute__((export_name("result_len"))) i32 result_len(void) {
    return (i32)out_len;
}

__attribute__((export_name("manifest"))) i32 manifest(void) {
    const char *json =
        "{\"name\":\"c-ticker\",\"version\":\"0.1.0\","
        "\"description\":\"freestanding C widget (no libc, no WASI)\","
        "\"author\":\"xtop-cli\",\"max_processes\":1,\"api\":\"1\"}";
    out_len = slen(json);
    memcpy(out, json, out_len);
    return (i32)(unsigned long)(void *)out;
}

__attribute__((export_name("render"))) i32 render(i32 state_ptr, i32 state_len) {
    if (state_ptr == 0 || state_len <= 0) {
        return 0;
    }
    const char *state = (const char *)(unsigned long)(u32)state_ptr;
    u32 len = (u32)state_len;

    u64 tick = find_u64(state, len, "\"tick\":");
    u64 uptime = find_u64(state, len, "\"uptime\":");
    u64 width = find_u64(state, len, "\"width\":");
    u64 height = find_u64(state, len, "\"height\":");

    u32 pos = 0;
    pos = put_str(out, pos, "{\"ops\":[");
    if (width >= 12 && height >= 4) {
        u64 inner = width - 2;
        pos = put_str(out, pos,
                      "{\"op\":\"block\",\"rect\":{\"x\":0,\"y\":0,\"width\":");
        pos = put_u64(out, pos, width);
        pos = put_str(out, pos, ",\"height\":");
        pos = put_u64(out, pos, height);
        pos = put_str(out, pos,
                      "},\"border\":\"rounded\",\"title\":\"c wasm\"},");
        pos = put_str(out, pos,
                      "{\"op\":\"text\",\"rect\":{\"x\":1,\"y\":1,\"width\":");
        pos = put_u64(out, pos, inner);
        pos = put_str(out, pos,
                      ",\"height\":1},\"spans\":[{\"text\":\"tick ");
        pos = put_u64(out, pos, tick);
        pos = put_str(out, pos,
                      "\",\"bold\":true,\"fg\":[252,97,141]}],\"align\":\"center\"},");
        pos = put_str(out, pos,
                      "{\"op\":\"text\",\"rect\":{\"x\":1,\"y\":2,\"width\":");
        pos = put_u64(out, pos, inner);
        pos = put_str(out, pos,
                      ",\"height\":1},\"spans\":[{\"text\":\"uptime ");
        pos = put_u64(out, pos, uptime);
        pos = put_str(out, pos, "s\"}],\"align\":\"center\"}");
    }
    pos = put_str(out, pos, "]}");
    out_len = pos;
    return (i32)(unsigned long)(void *)out;
}
