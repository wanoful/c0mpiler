typedef unsigned long size_t;
typedef unsigned int uint32_t;
typedef int int32_t;

long sys_write(int fd, const char *buf, size_t len);
long sys_read(int fd, char *buf, size_t len);
void *sys_brk(void *addr);
void sys_exit(int code) __attribute__((noreturn));

void my_putchar(int c) {
    char ch = (char)c;
    sys_write(1, &ch, 1);
}

int my_getchar(void) {
    char c;
    long n = sys_read(0, &c, 1);
    if (n == 1)
        return (unsigned char)c;
    return -1;
}

size_t my_strlen(const char *s) {
    size_t len = 0;
    while (*s++) len++;
    return len;
}

static char *heap_brk;
void *my_malloc(size_t size) {
    if (!heap_brk) {
        heap_brk = (char *)sys_brk((void *)0);
    }
    char *ptr = heap_brk;
    char *new_brk = (char *)sys_brk((void *)(ptr + size));
    if (new_brk < ptr + size)
        return (void *)0;
    heap_brk = ptr + size;
    return (void *)ptr;
}

void *my_realloc(void *ptr, size_t size) {
    char *old = (char *)ptr;
    char *new = (char *)my_malloc(size);
    if (new) {
        for (size_t i = 0; i < size; i++)
            new[i] = old[i];
    }
    return (void *)new;
}

static void print_int(int n) {
    unsigned int un;
    if (n < 0) {
        my_putchar('-');
        un = -(unsigned int)n;
    } else {
        un = (unsigned int)n;
    }
    char buf[12];
    int i = 0;
    do {
        buf[i++] = '0' + (un % 10);
        un /= 10;
    } while (un);
    while (i > 0)
        my_putchar(buf[--i]);
}

void my_printf(const char *fmt, int arg) {
    for (const char *p = fmt; *p; p++) {
        if (*p == '%') {
            p++;
            if (*p == 'd') {
                print_int(arg);
            }
        } else {
            my_putchar(*p);
        }
    }
}

int my_sprintf(char *buf, const char *fmt, unsigned int arg) {
    int pos = 0;
    for (const char *p = fmt; *p; p++) {
        if (*p == '%') {
            p++;
            if (*p == 'u') {
                char tmp[12];
                int i = 0;
                unsigned int val = arg;
                do {
                    tmp[i++] = '0' + (val % 10);
                    val /= 10;
                } while (val);
                while (i > 0)
                    buf[pos++] = tmp[--i];
            }
        } else {
            buf[pos++] = *p;
        }
    }
    buf[pos] = '\0';
    return pos;
}

int my_scanf(const char *fmt, int *out) {
    char buf[32];
    int i = 0;
    char c;
    while (i < 31) {
        long n = sys_read(0, &c, 1);
        if (n != 1) break;
        if (c == '\n') break;
        buf[i++] = c;
    }
    buf[i] = '\0';
    int sign = 1;
    int val = 0;
    char *p = buf;
    if (*p == '-') { sign = -1; p++; }
    while (*p >= '0' && *p <= '9') {
        val = val * 10 + (*p - '0');
        p++;
    }
    *out = sign * val;
    return 1;
}

void *memcpy(void *dest, const void *src, unsigned long n) {
    char *d = (char *)dest;
    const char *s = (const char *)src;
    for (unsigned long i = 0; i < n; i++)
        d[i] = s[i];
    return dest;
}

void *memmove(void *dest, const void *src, unsigned long n) {
    char *d = (char *)dest;
    const char *s = (const char *)src;
    if (d < s) {
        for (unsigned long i = 0; i < n; i++)
            d[i] = s[i];
    } else {
        for (unsigned long i = n; i > 0; i--)
            d[i - 1] = s[i - 1];
    }
    return dest;
}
