// Inject one real fsync failure at a selected filesystem boundary.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdarg.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

static int progress_fault(int descriptor, const char *path) {
  const char *phase = getenv("PROGRESS_FAULT");
  const char *batch = getenv("PROGRESS_BATCH");
  if (phase == NULL || batch == NULL) return 0;
  int directory = strstr(phase, "directory") != NULL;
  char buffer[32768];
  ssize_t size;
  if (directory) {
    if (strcmp(path, batch) != 0) return 0;
    int file = openat(descriptor, "batch.json", O_RDONLY);
    if (file < 0) return 0;
    size = read(file, buffer, sizeof(buffer) - 1);
    close(file);
  } else {
    if (strncmp(path, batch, strlen(batch)) != 0 || strstr(path, "/.tmp") == NULL) return 0;
    size = pread(descriptor, buffer, sizeof(buffer) - 1, 0);
  }
  if (size < 0) return 0;
  buffer[size] = '\0';
  const char *status = strstr(phase, "graph-complete") != NULL ? "\"verified\": true" :
    (strstr(phase, "graph-first") != NULL ? "\"parent_links\": 1" :
    (strstr(phase, "authorization") != NULL ? "\"retry_authorized\"" :
    (strstr(phase, "intent") != NULL ? "\"creating\"" : "\"created\"")));
  if (strstr(phase, "graph-start") != NULL &&
      (strstr(buffer, "\"pending\"") != NULL || strstr(buffer, "\"creating\"") != NULL)) return 0;
  if (strstr(buffer, status) == NULL) return 0;
  int record = open("progress-fault-receipt", O_WRONLY | O_CREAT | O_EXCL, 0600);
  if (record < 0) return 0;
  close(record);
  const char *effect = getenv("PROGRESS_EFFECT");
  if (effect != NULL && strcmp(effect, "exit") == 0) _exit(91);
  errno = EIO;
  return -1;
}

int fsync(int descriptor) {
  int (*real_sync)(int) = (int (*)(int))dlsym(RTLD_NEXT, "fsync");
  if (real_sync == NULL) { errno = EIO; return -1; }
  const char *target = getenv("SYNC_FAULT");
  const char *receipt = getenv("SYNC_RECEIPT");
  char link[64];
  char path[PATH_MAX + 1];
  snprintf(link, sizeof(link), "/proc/self/fd/%d", descriptor);
  ssize_t length = readlink(link, path, PATH_MAX);
  if (length < 0) return real_sync(descriptor);
  path[length] = '\0';
  if (progress_fault(descriptor, path) != 0) return -1;
  if (target == NULL || receipt == NULL) return real_sync(descriptor);
  if (strcmp(path, target) != 0) return real_sync(descriptor);
  int record = open(receipt, O_WRONLY | O_CREAT | O_EXCL, 0600);
  if (record >= 0) {
    close(record);
    errno = EIO;
    return -1;
  }
  int result = real_sync(descriptor);
  if (result == 0) {
    record = open(receipt, O_WRONLY | O_APPEND);
    if (record >= 0) {
      const char message[] = "synchronized\n";
      (void)write(record, message, sizeof(message) - 1);
      close(record);
    }
  }
  return result;
}

// A traverse-only ancestor must not require a read-only directory descriptor.
static int checked_open(const char *symbol, const char *path, int flags, mode_t mode) {
  int (*real_open)(const char *, int, ...) = (int (*)(const char *, int, ...))dlsym(RTLD_NEXT, symbol);
  if (real_open == NULL) { errno = EIO; return -1; }
  const char *denied = getenv("DENY_READ_DIRECTORY");
  if (denied != NULL && strcmp(denied, path) == 0 && (flags & O_ACCMODE) == O_RDONLY && (flags & O_PATH) == 0) {
    errno = EACCES;
    return -1;
  }
  return real_open(path, flags, mode);
}

int open(const char *path, int flags, ...) {
  mode_t mode = 0;
  if ((flags & O_CREAT) != 0 || (flags & O_TMPFILE) == O_TMPFILE) {
    va_list args; va_start(args, flags); mode = va_arg(args, mode_t); va_end(args);
  }
  return checked_open("open", path, flags, mode);
}

int open64(const char *path, int flags, ...) {
  mode_t mode = 0;
  if ((flags & O_CREAT) != 0 || (flags & O_TMPFILE) == O_TMPFILE) {
    va_list args; va_start(args, flags); mode = va_arg(args, mode_t); va_end(args);
  }
  return checked_open("open64", path, flags, mode);
}
