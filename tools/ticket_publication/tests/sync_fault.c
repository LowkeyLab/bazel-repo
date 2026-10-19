// Inject one real fsync failure at a selected filesystem boundary.
#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <fcntl.h>
#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

int fsync(int descriptor) {
  int (*real_sync)(int) = (int (*)(int))dlsym(RTLD_NEXT, "fsync");
  if (real_sync == NULL) { errno = EIO; return -1; }
  const char *target = getenv("SYNC_FAULT");
  const char *receipt = getenv("SYNC_RECEIPT");
  char link[64];
  char path[PATH_MAX + 1];
  snprintf(link, sizeof(link), "/proc/self/fd/%d", descriptor);
  ssize_t length = readlink(link, path, PATH_MAX);
  if (target == NULL || receipt == NULL || length < 0) return real_sync(descriptor);
  path[length] = '\0';
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
