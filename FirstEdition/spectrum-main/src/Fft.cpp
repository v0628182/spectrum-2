#include "Fft.h"

#include <cmath>
#include <stdexcept>

#include "warzone_audio/Constants.h"

namespace warzone_audio {

Fft::Fft(unsigned size)
    : size_(size)
{
    if (size_ == 0 || (size_ & (size_ - 1)) != 0) {
        throw std::invalid_argument("FFT size must be a power of two");
    }
}

void Fft::forward(std::vector<std::complex<float>>& data) const
{
    const unsigned n = size_;
    if (data.size() != n) {
        throw std::invalid_argument("FFT data size mismatch");
    }

    unsigned j = 0;
    for (unsigned i = 1; i < n; ++i) {
        unsigned bit = n >> 1;
        while ((j & bit) != 0) {
            j ^= bit;
            bit >>= 1;
        }
        j ^= bit;
        if (i < j) {
            std::swap(data[i], data[j]);
        }
    }

    for (unsigned len = 2; len <= n; len <<= 1) {
        const float angle = -2.0f * constants::kPi / static_cast<float>(len);
        const std::complex<float> wlen(std::cos(angle), std::sin(angle));

        for (unsigned i = 0; i < n; i += len) {
            std::complex<float> w(1.0f, 0.0f);
            for (unsigned k = 0; k < len / 2; ++k) {
                const auto u = data[i + k];
                const auto v = data[i + k + len / 2] * w;
                data[i + k] = u + v;
                data[i + k + len / 2] = u - v;
                w *= wlen;
            }
        }
    }
}

} // namespace warzone_audio
