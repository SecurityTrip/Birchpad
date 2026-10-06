#include <vector>
namespace demo {
template <typename T>
class Box {
public:
    explicit Box(T value) : value_(value) {}
    T get() const { return value_; } // accessor
private:
    T value_;
};
}

int countdown() {
    int n = 42;
loop:
    if (--n > 0) goto loop;
    return n;
}
