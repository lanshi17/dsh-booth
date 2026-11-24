#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/text_util.hpp"

using namespace booth;

TEST_CASE("ellipsize keeps graphemes and terminal cells intact") {
  REQUIRE(ellipsize("abc", 10) == "abc");
  REQUIRE(ellipsize("abcdef", 4) == "abc…");
  REQUIRE(ellipsize("中文路径", 5) == "中文…");
  REQUIRE(ellipsize("中文路径", 1) == "…");
  REQUIRE(ellipsize("中文路径", 0).empty());
}
