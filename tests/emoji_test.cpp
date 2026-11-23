#include <catch2/catch_test_macros.hpp>

#include "interaction/emoji.hpp"

TEST_CASE("emoji shortcodes expand and unknown ones stay") {
  REQUIRE(booth::expand_emojis("ship :rocket: now") == std::string("ship 🚀 now"));
  REQUIRE(booth::expand_emojis(":smile::tada:") == std::string("😄🎉"));
  REQUIRE(booth::expand_emojis(":unknown:") == std::string(":unknown:"));
  REQUIRE(booth::expand_emojis("time 12:30") == std::string("time 12:30"));
}
