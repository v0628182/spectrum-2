#pragma once

#include <array>
#include <cstddef>
#include <cstdint>

namespace warzone_audio {

enum class SpeakerRole {
    Unknown,
    FrontLeft,
    FrontRight,
    FrontCenter,
    LowFrequency,
    BackLeft,
    BackRight,
    FrontLeftOfCenter,
    FrontRightOfCenter,
    BackCenter,
    SideLeft,
    SideRight,
};

struct SpatialLayout {
    std::array<SpeakerRole, 8> roles{};
    std::size_t channels = 0;
    std::uint32_t mask = 0;

    int indexOf(SpeakerRole role) const
    {
        for (std::size_t i = 0; i < channels && i < roles.size(); ++i) {
            if (roles[i] == role) {
                return static_cast<int>(i);
            }
        }
        return -1;
    }

    int frontLeft() const { return indexOf(SpeakerRole::FrontLeft); }
    int frontRight() const { return indexOf(SpeakerRole::FrontRight); }
    int frontCenter() const { return indexOf(SpeakerRole::FrontCenter); }
    int lowFrequency() const { return indexOf(SpeakerRole::LowFrequency); }

    bool isFront(std::size_t idx) const
    {
        if (idx >= channels || idx >= roles.size()) {
            return false;
        }
        const auto role = roles[idx];
        return role == SpeakerRole::FrontLeft || role == SpeakerRole::FrontRight ||
               role == SpeakerRole::FrontCenter || role == SpeakerRole::FrontLeftOfCenter ||
               role == SpeakerRole::FrontRightOfCenter;
    }

    bool isSideOrBack(std::size_t idx) const
    {
        if (idx >= channels || idx >= roles.size()) {
            return false;
        }
        const auto role = roles[idx];
        return role == SpeakerRole::BackLeft || role == SpeakerRole::BackRight ||
               role == SpeakerRole::BackCenter || role == SpeakerRole::SideLeft ||
               role == SpeakerRole::SideRight;
    }
};

namespace speaker_mask {
constexpr std::uint32_t frontLeft = 0x1;
constexpr std::uint32_t frontRight = 0x2;
constexpr std::uint32_t frontCenter = 0x4;
constexpr std::uint32_t lowFrequency = 0x8;
constexpr std::uint32_t backLeft = 0x10;
constexpr std::uint32_t backRight = 0x20;
constexpr std::uint32_t frontLeftOfCenter = 0x40;
constexpr std::uint32_t frontRightOfCenter = 0x80;
constexpr std::uint32_t backCenter = 0x100;
constexpr std::uint32_t sideLeft = 0x200;
constexpr std::uint32_t sideRight = 0x400;
} // namespace speaker_mask

inline SpeakerRole roleFromMaskBit(std::uint32_t bit)
{
    switch (bit) {
    case speaker_mask::frontLeft:
        return SpeakerRole::FrontLeft;
    case speaker_mask::frontRight:
        return SpeakerRole::FrontRight;
    case speaker_mask::frontCenter:
        return SpeakerRole::FrontCenter;
    case speaker_mask::lowFrequency:
        return SpeakerRole::LowFrequency;
    case speaker_mask::backLeft:
        return SpeakerRole::BackLeft;
    case speaker_mask::backRight:
        return SpeakerRole::BackRight;
    case speaker_mask::frontLeftOfCenter:
        return SpeakerRole::FrontLeftOfCenter;
    case speaker_mask::frontRightOfCenter:
        return SpeakerRole::FrontRightOfCenter;
    case speaker_mask::backCenter:
        return SpeakerRole::BackCenter;
    case speaker_mask::sideLeft:
        return SpeakerRole::SideLeft;
    case speaker_mask::sideRight:
        return SpeakerRole::SideRight;
    default:
        return SpeakerRole::Unknown;
    }
}

inline SpatialLayout fallbackLayout(std::size_t channels)
{
    SpatialLayout layout;
    layout.channels = channels > layout.roles.size() ? layout.roles.size() : channels;
    layout.roles.fill(SpeakerRole::Unknown);

    switch (layout.channels) {
    case 1:
        layout.roles[0] = SpeakerRole::FrontCenter;
        break;
    case 2:
        layout.roles[0] = SpeakerRole::FrontLeft;
        layout.roles[1] = SpeakerRole::FrontRight;
        break;
    case 6:
        layout.roles[0] = SpeakerRole::FrontLeft;
        layout.roles[1] = SpeakerRole::FrontRight;
        layout.roles[2] = SpeakerRole::FrontCenter;
        layout.roles[3] = SpeakerRole::LowFrequency;
        layout.roles[4] = SpeakerRole::BackLeft;
        layout.roles[5] = SpeakerRole::BackRight;
        break;
    case 8:
        layout.roles[0] = SpeakerRole::FrontLeft;
        layout.roles[1] = SpeakerRole::FrontRight;
        layout.roles[2] = SpeakerRole::FrontCenter;
        layout.roles[3] = SpeakerRole::LowFrequency;
        layout.roles[4] = SpeakerRole::BackLeft;
        layout.roles[5] = SpeakerRole::BackRight;
        layout.roles[6] = SpeakerRole::SideLeft;
        layout.roles[7] = SpeakerRole::SideRight;
        break;
    default:
        if (layout.channels > 0) {
            layout.roles[0] = SpeakerRole::FrontLeft;
        }
        if (layout.channels > 1) {
            layout.roles[1] = SpeakerRole::FrontRight;
        }
        for (std::size_t i = 2; i < layout.channels; ++i) {
            layout.roles[i] = SpeakerRole::Unknown;
        }
        break;
    }

    return layout;
}

inline SpatialLayout resolveLayout(std::size_t channels, std::uint32_t mask)
{
    if (channels == 0) {
        return {};
    }

    if (mask == 0) {
        return fallbackLayout(channels);
    }

    SpatialLayout layout;
    layout.channels = channels > layout.roles.size() ? layout.roles.size() : channels;
    layout.mask = mask;
    layout.roles.fill(SpeakerRole::Unknown);

    const std::array<std::uint32_t, 11> bits{
        speaker_mask::frontLeft,
        speaker_mask::frontRight,
        speaker_mask::frontCenter,
        speaker_mask::lowFrequency,
        speaker_mask::backLeft,
        speaker_mask::backRight,
        speaker_mask::frontLeftOfCenter,
        speaker_mask::frontRightOfCenter,
        speaker_mask::backCenter,
        speaker_mask::sideLeft,
        speaker_mask::sideRight,
    };

    std::size_t out = 0;
    for (const auto bit : bits) {
        if ((mask & bit) != 0 && out < layout.channels) {
            layout.roles[out++] = roleFromMaskBit(bit);
        }
    }

    if (out == 0) {
        return fallbackLayout(channels);
    }

    while (out < layout.channels) {
        layout.roles[out++] = SpeakerRole::Unknown;
    }
    return layout;
}

inline float channelOrZero(const float* frame, int index)
{
    return index >= 0 ? frame[index] : 0.0f;
}

} // namespace warzone_audio
