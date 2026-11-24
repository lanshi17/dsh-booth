#include <catch2/catch_test_macros.hpp>

#include <string>

#include <ftxui/screen/string.hpp>

#include "interaction/text_util.hpp"

using namespace booth;

TEST_CASE("ellipsize keeps graphemes and terminal cells intact") {
  REQUIRE(ellipsize("abc", 10) == "abc");
  REQUIRE(ellipsize("abcdef", 4) == "abc…");
  REQUIRE(ellipsize("中文路径", 5) == "中文…");
  REQUIRE(ellipsize("中文路径", 1) == "…");
  REQUIRE(ellipsize("中文路径", 0).empty());
}

TEST_CASE("wrap_cells counts terminal cells and expands tabs") {
  const auto rows = wrap_cells("中文中文", 4);
  REQUIRE(rows.size() == 2);
  REQUIRE(rows[0] == "中文");
  REQUIRE(rows[1] == "中文");
  REQUIRE(ftxui::string_width(rows[0]) == 4);
  const auto tabs = wrap_cells("a\tb", 10);
  REQUIRE(tabs.size() == 1);
  REQUIRE(tabs[0] == "a    b");
}
