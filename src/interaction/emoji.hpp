#pragma once

#include <string>
#include <string_view>

namespace booth {

/// Replaces `:shortcode:` tokens with their emoji; unknown tokens stay verbatim.
std::string expand_emojis(std::string_view text);

}  // namespace booth
