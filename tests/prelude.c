typedef unsigned int uint32_t;
typedef int int32_t;
typedef unsigned long size_t;

extern int sprintf(char *str, const char *format, ...);
extern int printf(const char *format, ...);
extern void *malloc(size_t size);
extern void *realloc(void *ptr, size_t size);
extern int scanf(const char *format, ...);
extern int getchar(void);
extern int putchar(int c);
extern size_t strlen(const char *s);

struct String {
  char *data;
  size_t length;
};

struct FatPtr {
  char *data;
  size_t length;
};

typedef struct String String;

void to_string(String *string, uint32_t *self) {
  char *buffer = malloc(16);
  sprintf(buffer, "%u", *self);
  size_t length = strlen(buffer);
  string->length = length;
  string->data = buffer;
}

void string_plus(String *ret, String *self, char *data, uint32_t length) {
  size_t new_length = self->length + length;
  char *new_data = malloc(new_length);
  for (size_t i = 0; i < self->length; i++)
    new_data[i] = self->data[i];
  for (size_t i = 0; i < length; i++)
    new_data[i + self->length] = data[i];
  ret->data = new_data;
  ret->length = new_length;
}

void print(char *text, uint32_t n) {
  for (uint32_t i = 0; i < n; i++)
    putchar(text[i]);
}

void println(char *text, uint32_t n) {
  print(text, n);
  putchar('\n');
}

void printInt(int32_t n) { printf("%d", n); }

void printlnInt(int32_t n) { printf("%d\n", n); }

void getString(String *string) {
  size_t capacity = 16;
  char *buffer = malloc(capacity);
  size_t length = 0;
  int c;

  while ((c = getchar()) != '\n' && c != -1) {
    if (length >= capacity) {
      capacity *= 2;
      char *new_buffer = realloc(buffer, capacity);
    }
    buffer[length++] = (char)c;
  }

  string->data = buffer;
  string->length = length;
}

int32_t getInt() {
  int32_t n;
  scanf("%d", &n);
  return n;
}

void string_as_str(struct FatPtr *ptr, String *self) {
  ptr->data = self->data;
  ptr->length = self->length;
}

size_t string_len(String *self) { return self->length; }
