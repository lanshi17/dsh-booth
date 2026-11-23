#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>

#include "interaction/theme.hpp"

using namespace booth;

TEST_CASE("theme colors parse only full hex values") {
  REQUIRE(parse_color("#00afff").has_value());
  REQUIRE_FALSE(parse_color("#00af").has_value());
  REQUIRE_FALSE(parse_color("blue").has_value());
  REQUIRE_FALSE(parse_color("#gggggg").has_value());
}

TEST_CASE("theme files expose only valid overrides") {
  const auto path = std::filesystem::temp_directory_path() /
                    ("booth-theme-" + std::to_string(::getpid()));
  {
    std::ofstream out(path);
    out << "accent=#ff0000\n"
        << "unknown=#123456\n"
        << "text=oops\n";
  }
  const auto theme = load_theme(path);
  REQUIRE(theme.size() == 2);
  REQUIRE(theme.at("accent") == "#ff0000");
  apply_theme(theme);
  REQUIRE(theme_color("accent", parse_color("#000000").value()) ==
          parse_color("#ff0000").value());
  unsetenv("BOOTH_THEME_ACCENT");
  std::filesystem::remove(path);
}
