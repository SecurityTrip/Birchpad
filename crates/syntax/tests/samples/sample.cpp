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
