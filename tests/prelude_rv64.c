#include <stdio.h>
#include <stdlib.h>
#include <string.h>

typedef unsigned int uint32_t;
typedef int int32_t;
typedef unsigned long size_t;

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
  char *buffer = malloc(16);
  sprintf(buffer, "%u", *self);
  uint32_t length = strlen(buffer);
  string->length = length;
  string->data = buffer;
}

void string_plus(String *ret, String *self, char *data, uint32_t length) {
  uint32_t new_length = self->length + length;
  char *new_data = malloc(new_length);
  for (int i = 0; i < self->length; i++)
    new_data[i] = self->data[i];
  for (int i = 0; i < length; i++)
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
  char *buffer = malloc(4096);
  uint32_t length = 0;
  int c;
  while ((c = getchar()) != '\n' && c != -1)
    buffer[length++] = (char)c;
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

uint32_t string_len(String *self) { return self->length; }
