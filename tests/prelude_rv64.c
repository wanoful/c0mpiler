typedef unsigned int uint32_t;
typedef int int32_t;
typedef unsigned long size_t;
typedef unsigned long uint64_t;

void *my_malloc(size_t size);
void *my_realloc(void *ptr, size_t size);
int my_printf(const char *format, ...);
int my_sprintf(char *str, const char *format, ...);
int my_scanf(const char *format, ...);
int my_getchar(void);
int my_putchar(int c);
size_t my_strlen(const char *s);

struct String {
  char *data;
  uint32_t length;
};

struct FatPtr {
  char *data;
  uint32_t length;
};

typedef struct String String;

void to_string(String *string, uint32_t *self) {
  char *buffer = my_malloc(16);
  my_sprintf(buffer, "%u", *self);
  uint32_t length = my_strlen(buffer);
  string->length = length;
  string->data = buffer;
}

void string_plus(String *ret, String *self, char *data, uint32_t length) {
  uint32_t new_length = self->length + length;
  char *new_data = my_malloc(new_length);
  for (int i = 0; i < self->length; i++) {
    new_data[i] = self->data[i];
  }
  for (int i = 0; i < length; i++) {
    new_data[i + self->length] = data[i];
  }
  ret->data = new_data;
  ret->length = new_length;
}

void print(char *text, uint32_t n) {
  for (uint32_t i = 0; i < n; i++) {
    my_putchar(text[i]);
  }
}

void println(char *text, uint32_t n) {
  print(text, n);
  my_putchar('\n');
}

void printInt(int32_t n) { my_printf("%d", n); }

void printlnInt(int32_t n) { my_printf("%d\n", n); }

void getString(String *string) {
  char *buffer = my_malloc(4096);
  uint32_t length = 0;
  int c;

  while ((c = my_getchar()) != '\n' && c != -1) {
    buffer[length++] = (char)c;
  }

  string->data = buffer;
  string->length = length;
}

int32_t getInt() {
  int32_t n;
  my_scanf("%d", &n);
  return n;
}

void string_as_str(struct FatPtr *ptr, String *self) {
  ptr->data = self->data;
  ptr->length = self->length;
}

uint32_t string_len(String *self) { return self->length; }
