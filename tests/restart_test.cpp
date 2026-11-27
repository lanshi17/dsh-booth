#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>

#include <unistd.h>

#include "interaction/restart.hpp"

namespace {

std::filesystem::path temp_file() {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-restart-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir);
  return dir / "restart.json";
}

}  // namespace

TEST_CASE("restart markers round-trip and are consumed once") {
  const auto file = temp_file();
  REQUIRE_FALSE(booth::take_restart_marker(file).has_value());
  std::string error;
  REQUIRE(booth::write_restart_marker(file, {"s-1", "/tmp", "title"}, error));
  const auto marker = booth::take_restart_marker(file);
  REQUIRE(marker.has_value());
  REQUIRE(marker->session == "s-1");
  REQUIRE(marker->cwd == "/tmp");
  REQUIRE(marker->title == "title");
  REQUIRE_FALSE(booth::take_restart_marker(file).has_value());
  std::filesystem::remove_all(file.parent_path());
}

TEST_CASE("malformed restart markers are discarded") {
  const auto file = temp_file();
  std::ofstream(file) << "{not json";
  REQUIRE_FALSE(booth::take_restart_marker(file).has_value());
  REQUIRE_FALSE(std::filesystem::exists(file));
  std::filesystem::remove_all(file.parent_path());
}
