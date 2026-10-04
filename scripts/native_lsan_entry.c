// Linux-only test adapter: link with --wrap=main to preserve the Koven IR.
#include <pthread.h>
#include <stdio.h>
#include <stdlib.h>

extern int __real_main(void);

static void fail(const char *operation, int status) {
    fprintf(stderr, "LSan fixture %s failed: %d\n", operation, status);
    // A broken harness must not trigger an exit-time leak report as acceptance.
    _Exit(90);
}

static void *run_koven(void *argument) {
    int *status = argument;
    *status = __real_main();
    return NULL;
}

int __wrap_main(void) {
    int entry_status = 0;
    pthread_t worker;
    int status = pthread_create(&worker, NULL, run_koven, &entry_status);
    if (status != 0) fail("pthread_create", status);
    // Joining retires the stack containing Koven pointers before LSan scans
    // roots at process exit. No pointer escapes through the thread result.
    status = pthread_join(worker, NULL);
    if (status != 0) fail("pthread_join", status);
    return entry_status;
}
