#include "logi_vhid.h"
#include <stddef.h>
#include <stdio.h>

int main(void) {
    const char *error = logi_vhid_last_error();
    if (error == NULL) {
        fputs("last_error returned NULL\n", stderr);
        return 1;
    }
    /* NULL is rejected before any device access. Never open or send input. */
    if (logi_vhid_pid_gate(NULL) != -1) {
        fputs("pid_gate(NULL) did not reject the null handle\n", stderr);
        return 1;
    }
    error = logi_vhid_last_error();
    if (error == NULL || error[0] == '\0') {
        fputs("null handle did not produce error text\n", stderr);
        return 1;
    }
    puts("C installed SDK safe smoke passed");
    return 0;
}
