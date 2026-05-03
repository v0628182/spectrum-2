#pragma once

#include <complex>
#include <vector>

namespace warzone_audio {

class Fft {
public:
    explicit Fft(unsigned size);

    void forward(std::vector<std::complex<float>>& data) const;
    unsigned size() const { return size_; }

private:
    unsigned size_;
};

} // namespace warzone_audio
