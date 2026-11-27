#include "interaction/recorder.hpp"

#include <filesystem>
#include <fstream>
#include <string>

#include <catch2/catch_test_macros.hpp>
#include <ftxui/dom/elements.hpp>

namespace {

std::string read_all(const std::filesystem::path& path) {
  std::ifstream file(path);
  return {std::istreambuf_iterator<char>(file), std::istreambuf_iterator<char>()};
}

}  // namespace

TEST_CASE("recorder writes start frame and stop rows") {
  const auto path = std::filesystem::temp_directory_path() / "booth-recorder-test.jsonl";
  std::filesystem::remove(path);
  std::string error;
  REQUIRE(booth::start_recording(path, error));
  REQUIRE(booth::recording());
  booth::record_frame(ftxui::text("recorder hello"));
  REQUIRE(booth::recorded_frames() == 1);
  booth::stop_recording();
  REQUIRE_FALSE(booth::recording());
  const std::string text = read_all(path);
  REQUIRE(text.find("\"type\":\"start\"") != std::string::npos);
  REQUIRE(text.find("\"type\":\"frame\"") != std::string::npos);
  REQUIRE(text.find("recorder hello") != std::string::npos);
  REQUIRE(text.find("\"type\":\"stop\"") != std::string::npos);
  std::filesystem::remove(path);
}
