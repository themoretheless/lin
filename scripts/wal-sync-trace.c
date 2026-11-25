#define _GNU_SOURCE
#include <dlfcn.h>
#include <errno.h>
#include <stdio.h>
#include <sys/stat.h>
#include <time.h>
#include <unistd.h>

static int trace_sync(int fd, const char *operation, int (*sync_file)(int)) {
    struct timespec start, end;
    clock_gettime(CLOCK_MONOTONIC, &start);
    int result = sync_file(fd);
    int saved_errno = errno;
    clock_gettime(CLOCK_MONOTONIC, &end);
    struct stat info;
    char link[64], path[4096];
    snprintf(link, sizeof(link), "/proc/self/fd/%d", fd);
    ssize_t length = readlink(link, path, sizeof(path) - 1);
    if (length >= 0 && fstat(fd, &info) == 0) {
        path[length] = '\0';
        long long ns = (end.tv_sec - start.tv_sec) * 1000000000LL
                     + end.tv_nsec - start.tv_nsec;
        dprintf(STDERR_FILENO, "WAL_SYNC %s %d %lld %lld %s\n",
                operation, result, (long long)info.st_size, ns, path);
    }
    errno = saved_errno;
    return result;
}

int fdatasync(int fd) {
    return trace_sync(fd, "fdatasync", dlsym(RTLD_NEXT, "fdatasync"));
}

int fsync(int fd) {
    return trace_sync(fd, "fsync", dlsym(RTLD_NEXT, "fsync"));
}
