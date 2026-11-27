#include "interaction/recorder_play.hpp"

#include <filesystem>
#include <fstream>
#include <sstream>
#include <string>

#include <catch2/catch_test_macros.hpp>
#include <nlohmann/json.hpp>

namespace {

std::filesystem::path write_recording(const std::string& body) {
  const auto path = std::filesystem::temp_directory_path() / "booth-play-test.jsonl";
  std::ofstream file(path, std::ios::trunc);
  file << body;
  return path;
}

}  // namespace

TEST_CASE("play_recording prints every frame") {
  const auto path = write_recording(
      nlohmann::json{{"type", "start"}, {"t", 0}}.dump() + "\n" +
      nlohmann::json{{"type", "frame"}, {"t", 0}, {"w", 5}, {"h", 1}, {"screen", "first"}}.dump() + "\n" +
      nlohmann::json{{"type", "frame"}, {"t", 10}, {"w", 5}, {"h", 1}, {"screen", "second"}}.dump() + "\n" +
      nlohmann::json{{"type", "stop"}, {"t", 20}, {"frames", 2}}.dump() + "\n");
  std::ostringstream out;
  std::ostringstream error;
  REQUIRE(booth::play_recording(path, 0, out, error) == 0);
  REQUIRE(error.str().empty());
  REQUIRE(out.str().find("first") != std::string::npos);
  REQUIRE(out.str().find("second") != std::string::npos);
  std::filesystem::remove(path);
}

TEST_CASE("play_recording rejects a file without frames") {
  const auto path = write_recording(
      nlohmann::json{{"type", "start"}, {"t", 0}}.dump() + "\n" +
      nlohmann::json{{"type", "stop"}, {"t", 0}, {"frames", 0}}.dump() + "\n");
  std::ostringstream out;
  std::ostringstream error;
  REQUIRE(booth::play_recording(path, 0, out, error) == 1);
  REQUIRE(error.str().find("no frames") != std::string::npos);
  std::filesystem::remove(path);
}
