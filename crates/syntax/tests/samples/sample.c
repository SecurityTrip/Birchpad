#include <stdio.h>
/* Entry point. */
int main(int argc, char **argv) {
    const char *name = "world";
    printf("Hello, %s!\n", name);
    return 0;
}

/* Retries until it gives up. */
static int retry(int times) {
    int tries = 0;
again:
    if (tries++ < times) goto again;
    return tries;
}
