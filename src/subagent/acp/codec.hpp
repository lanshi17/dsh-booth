#pragma once

#include <cstddef>
#include <string>
#include <string_view>

#include <nlohmann/json.hpp>

#include "contract/error.hpp"

namespace booth::acp {

using Json = nlohmann::json;

/// One frame larger than this is refused instead of buffered forever.
inline constexpr std::size_t kMaxLine = 16u * 1024u * 1024u;

/// Parse one newline-delimited frame. Accepts a trailing CRLF.
Result<Json> decode(std::string_view line);

/// Encode one frame, including its terminating newline.
std::string encode(const Json& message);

}  // namespace booth::acp
