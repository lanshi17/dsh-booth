#include <catch2/catch_test_macros.hpp>

#include <filesystem>

#include <unistd.h>

#include "interaction/session_pins.hpp"

using namespace booth;

TEST_CASE("session pins round-trip and remove ids") {
  const auto path = std::filesystem::temp_directory_path() /
                    ("booth-pins-" + std::to_string(::getpid()) + ".txt");
  std::filesystem::remove(path);
  REQUIRE(load_session_pins(path).empty());
  REQUIRE(set_session_pin(path, "s-1", true));
  REQUIRE(set_session_pin(path, "s-2", true));
  REQUIRE(load_session_pins(path) == std::set<std::string>{"s-1", "s-2"});
  REQUIRE(set_session_pin(path, "s-1", false));
  REQUIRE(load_session_pins(path) == std::set<std::string>{"s-2"});
  std::filesystem::remove(path);
}
