#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>

#include <unistd.h>

#include "interaction/preset_store.hpp"

namespace {

std::filesystem::path temp_file() {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-preset-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir);
  return dir / "model-presets.json";
}

}  // namespace

TEST_CASE("model presets save, list, find and remove") {
  const auto file = temp_file();
  REQUIRE(booth::load_model_presets(file).empty());
  REQUIRE(booth::model_preset_list(file).find("no model presets") != std::string::npos);

  booth::ModelPreset preset{"fast", "opencode/fast", "Fast", "reasoning_effort", "low", "Low"};
  REQUIRE(booth::save_model_preset(file, preset));
  REQUIRE(booth::load_model_presets(file).size() == 1);
  const auto found = booth::find_model_preset(file, "fast");
  REQUIRE(found.has_value());
  REQUIRE(found->model_value == "opencode/fast");
  REQUIRE(found->effort_value == "low");
  REQUIRE(booth::model_preset_list(file).find("fast") != std::string::npos);

  preset.effort_value = "high";
  preset.effort_name = "High";
  REQUIRE(booth::save_model_preset(file, preset));
  REQUIRE(booth::load_model_presets(file).size() == 1);
  REQUIRE(booth::find_model_preset(file, "fast")->effort_value == "high");

  REQUIRE(booth::remove_model_preset(file, "fast"));
  REQUIRE_FALSE(booth::remove_model_preset(file, "fast"));
  REQUIRE(booth::load_model_presets(file).empty());
  std::filesystem::remove_all(file.parent_path());
}

TEST_CASE("a malformed preset file reads as empty") {
  const auto file = temp_file();
  std::ofstream(file) << "{not json";
  REQUIRE(booth::load_model_presets(file).empty());
  REQUIRE(booth::save_model_preset(file, {"one", "m", "M", "", "", ""}));
  REQUIRE(booth::load_model_presets(file).size() == 1);
  std::filesystem::remove_all(file.parent_path());
}
