#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>

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
  REQUIRE(theme.size() == 1);
  REQUIRE(theme.at("accent") == "#ff0000");
  apply_theme(theme);
  REQUIRE(theme_color("accent", parse_color("#000000").value()) ==
          parse_color("#ff0000").value());
  unsetenv("BOOTH_THEME_ACCENT");
  std::filesystem::remove(path);
}

TEST_CASE("theme overrides update one value and preserve comments and unrelated lines") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-theme-update-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto path = dir / "theme.conf";
  {
    std::ofstream out(path);
    out << "# custom palette\n"
        << "accent = #ff0000  # primary\n"
        << "text=#ffffff\n"
        << "some unrelated setting\n";
  }

  const auto result = update_theme_override(path, "accent=#123456");
  REQUIRE(result.success);
  REQUIRE(result.key == "accent");
  REQUIRE(result.color == "#123456");

  std::ifstream input(path);
  const std::string contents((std::istreambuf_iterator<char>(input)),
                             std::istreambuf_iterator<char>());
  REQUIRE(contents ==
          "# custom palette\n"
          "accent = #123456  # primary\n"
          "text=#ffffff\n"
          "some unrelated setting\n");
  REQUIRE(load_theme(path).at("accent") == "#123456");
  std::filesystem::remove_all(dir);
}

TEST_CASE("invalid theme assignments never modify the theme file") {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-theme-invalid-" + std::to_string(::getpid()));
  std::filesystem::create_directories(dir);
  const auto path = dir / "theme.conf";
  {
    std::ofstream out(path);
    out << "# keep me\naccent=#abcdef\n";
  }
  std::ifstream before_input(path);
  const std::string before((std::istreambuf_iterator<char>(before_input)),
                           std::istreambuf_iterator<char>());

  for (const std::string assignment : {"unknown=#123456", "accent=oops", "accent"}) {
    const auto result = update_theme_override(path, assignment);
    REQUIRE_FALSE(result.success);
    REQUIRE_FALSE(result.error.empty());
    std::ifstream input(path);
    const std::string contents((std::istreambuf_iterator<char>(input)),
                               std::istreambuf_iterator<char>());
    REQUIRE(contents == before);
  }
  std::filesystem::remove_all(dir);
}
