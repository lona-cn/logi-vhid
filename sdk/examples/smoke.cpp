#include "logi_vhid.h"
#include <iostream>

int main() {
    const char *error = logi_vhid_last_error();
    if (error == nullptr) {
        std::cerr << "last_error returned nullptr\n";
        return 1;
    }
    // nullptr is rejected before any device access. Never open or send input.
    if (logi_vhid_pid_gate(nullptr) != -1) {
        std::cerr << "pid_gate(nullptr) did not reject the null handle\n";
        return 1;
    }
    error = logi_vhid_last_error();
    if (error == nullptr || error[0] == '\0') {
        std::cerr << "null handle did not produce error text\n";
        return 1;
    }
    std::cout << "C++ installed SDK safe smoke passed\n";
    return 0;
}
