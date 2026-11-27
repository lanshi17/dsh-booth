#include "interaction/changelog.hpp"

#include <string>

#include <catch2/catch_test_macros.hpp>

TEST_CASE("parse_changelog reads version sections newest first") {
  const auto entries = booth::parse_changelog(
      "# Changelog\n\n"
      "## [1.2.3] — 2026-01-02\n\n"
      "### Added\n- one\n\n"
      "## [1.1.0] - 2025-12-01\n\n"
      "- two\n");
  REQUIRE(entries.size() == 2);
  REQUIRE(entries[0].major == 1);
  REQUIRE(entries[0].minor == 2);
  REQUIRE(entries[0].patch == 3);
  REQUIRE(entries[0].content.find("## [1.2.3]") == 0);
  REQUIRE(entries[0].content.find("- one") != std::string::npos);
  REQUIRE(entries[1].content.find("- two") != std::string::npos);
}

TEST_CASE("format_changelog selects recent, last and full views") {
  const auto entries = booth::parse_changelog(
      "## [4.0.0] - d\n- four\n\n"
      "## [3.0.0] - d\n- three\n\n"
      "## [2.0.0] - d\n- two\n\n"
      "## [1.0.0] - d\n- one\n");
  REQUIRE(entries.size() == 4);
  const auto recent = booth::format_changelog("", entries);
  REQUIRE(recent.find("4.0.0") != std::string::npos);
  REQUIRE(recent.find("1.0.0") == std::string::npos);
  const auto last = booth::format_changelog("last", entries);
  REQUIRE(last.find("4.0.0") != std::string::npos);
  REQUIRE(last.find("3.0.0") == std::string::npos);
  const auto two = booth::format_changelog("last 2", entries);
  REQUIRE(two.find("3.0.0") != std::string::npos);
  REQUIRE(two.find("2.0.0") == std::string::npos);
  REQUIRE(two.find("1.0.0") == std::string::npos);
  const auto full = booth::format_changelog("full", entries);
  REQUIRE(full.find("1.0.0") != std::string::npos);
  REQUIRE(booth::format_changelog("last 0", entries).find("Usage") == 0);
  REQUIRE(booth::format_changelog("nope", entries).find("Usage") == 0);
}
