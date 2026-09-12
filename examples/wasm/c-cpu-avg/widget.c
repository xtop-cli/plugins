// Freestanding C runtime widget for xtop (wasm32, no libc, no WASI).
//
// c-cpu-avg scans every `"usage":` value out of the State JSON (the CPUs
// array), computes count/mean/min/max and draws one bar per core plus a
// gauge and a footer. One decimal digit is kept with fixed-point (x10)
// integer math, so no floating point or libm is needed.
//
// Build: ./build.sh

typedef unsigned int u32;
typedef unsigned long long u64;
typedef int i32;

#define MAX_CORES 64

static unsigned char heap[65536];
static u32 heap_used = 0;

static char out[16384];
static u32 out_len = 0;

// Per-core usage in tenths of a percent (x10).
static u32 cores[MAX_CORES];
static u32 core_count = 0;

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

// "12.3" from a fixed-point x10 value.
static u32 put_fixed1(char *dst, u32 pos, u32 value_x10) {
    pos = put_u64(dst, pos, value_x10 / 10);
    dst[pos++] = '.';
    dst[pos++] = (char)('0' + (value_x10 % 10));
    return pos;
}

static u32 find_u64(const char *json, u32 len, const char *key) {
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
            return (u32)value;
        }
    }
    return 0;
}

// Collect every `"usage":` number in the JSON as fixed-point x10.
static void scan_usage(const char *json, u32 len) {
    const char *key = "\"usage\":";
    u32 key_len = slen(key);
    core_count = 0;
    for (u32 i = 0; i + key_len <= len && core_count < MAX_CORES; i++) {
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
        u32 value = 0;
        int digits = 0;
        while (j < len && json[j] >= '0' && json[j] <= '9') {
            value = value * 10 + (u32)(json[j] - '0');
            j++;
            digits = 1;
        }
        if (!digits) {
            continue;
        }
        if (j < len && json[j] == '.') {
            j++;
            if (j < len && json[j] >= '0' && json[j] <= '9') {
                value = value * 10 + (u32)(json[j] - '0');
            }
        } else {
            value *= 10;
        }
        cores[core_count++] = value;
    }
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
        "{\"name\":\"c-cpu-avg\",\"version\":\"0.1.0\","
        "\"description\":\"per-core CPU statistics (mean/min/max) in freestanding C\","
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

    scan_usage(state, len);
    u64 width = find_u64(state, len, "\"width\":");
    u64 height = find_u64(state, len, "\"height\":");

    u32 pos = 0;
    pos = put_str(out, pos, "{\"ops\":[");
    if (width >= 16 && height >= 6) {
        u64 inner = width - 2;
        u32 sum = 0;
        u32 min = 0;
        u32 max = 0;
        for (u32 i = 0; i < core_count; i++) {
            u32 value = cores[i];
            sum += value;
            if (i == 0 || value < min) {
                min = value;
            }
            if (i == 0 || value > max) {
                max = value;
            }
        }
        u32 avg = core_count > 0 ? sum / core_count : 0;

        pos = put_str(out, pos,
                      "{\"op\":\"block\",\"rect\":{\"x\":0,\"y\":0,\"width\":");
        pos = put_u64(out, pos, width);
        pos = put_str(out, pos, ",\"height\":");
        pos = put_u64(out, pos, height);
        pos = put_str(out, pos,
                      "},\"border\":\"rounded\",\"title\":\"c cpu avg\"},");

        // Gauge with the mean.
        pos = put_str(out, pos,
                      "{\"op\":\"gauge\",\"rect\":{\"x\":1,\"y\":1,\"width\":");
        pos = put_u64(out, pos, inner);
        pos = put_str(out, pos, ",\"height\":3},\"ratio\":");
        pos = put_fixed1(out, pos, avg);
        pos = put_str(out, pos, ",\"label\":\"avg ");
        pos = put_fixed1(out, pos, avg);
        pos = put_str(out, pos, "%\"},");

        // One bar per core, capped to the available rows.
        u64 bar_rows = height - 6;
        if (bar_rows > core_count) {
            bar_rows = core_count;
        }
        for (u32 i = 0; i < (u32)bar_rows; i++) {
            pos = put_str(out, pos,
                          "{\"op\":\"bar\",\"rect\":{\"x\":1,\"y\":");
            pos = put_u64(out, pos, 4 + i);
            pos = put_str(out, pos, ",\"width\":");
            pos = put_u64(out, pos, inner);
            pos = put_str(out, pos, ",\"height\":1},\"ratio\":");
            pos = put_fixed1(out, pos, cores[i]);
            pos = put_str(out, pos, ",\"label\":\"core ");
            pos = put_u64(out, pos, i);
            pos = put_str(out, pos, "  ");
            pos = put_fixed1(out, pos, cores[i]);
            pos = put_str(out, pos, "%\"},");
        }

        // Footer: count/mean/min/max/spread.
        pos = put_str(out, pos,
                      "{\"op\":\"text\",\"rect\":{\"x\":1,\"y\":");
        pos = put_u64(out, pos, height - 2);
        pos = put_str(out, pos, ",\"width\":");
        pos = put_u64(out, pos, inner);
        pos = put_str(out, pos,
                      ",\"height\":1},\"spans\":[{\"text\":\"cores ");
        pos = put_u64(out, pos, core_count);
        pos = put_str(out, pos, "   avg ");
        pos = put_fixed1(out, pos, avg);
        pos = put_str(out, pos, "   min ");
        pos = put_fixed1(out, pos, min);
        pos = put_str(out, pos, "   max ");
        pos = put_fixed1(out, pos, max);
        pos = put_str(out, pos, "   spread ");
        pos = put_fixed1(out, pos, max - min);
        pos = put_str(out, pos, "\"}],\"align\":\"center\"}");
    }
    pos = put_str(out, pos, "]}");
    out_len = pos;
    return (i32)(unsigned long)(void *)out;
}
