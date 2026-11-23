#include "interaction/shell_command.hpp"

#include <catch2/catch_test_macros.hpp>

#include <chrono>
#include <filesystem>
#include <string>

using namespace booth;

TEST_CASE("shell command captures stdout and stderr together") {
  const auto result = run_shell_command("printf out; printf err >&2", ".");
  REQUIRE(result.exit_code == 0);
  REQUIRE_FALSE(result.timed_out);
  REQUIRE(result.output == "outerr");
}

TEST_CASE("shell command reports nonzero exit status") {
  const auto result = run_shell_command("printf failed; exit 7", ".");
  REQUIRE(result.exit_code == 7);
  REQUIRE(result.output == "failed");
}

TEST_CASE("shell command runs in the requested working directory") {
  const auto cwd = std::filesystem::temp_directory_path();
  const auto result = run_shell_command("pwd", cwd.string());
  REQUIRE(result.exit_code == 0);
  std::string expected = cwd.string();
  while (!expected.empty() && expected.back() == '/') expected.pop_back();
  REQUIRE(result.output == expected + "\n");
}

TEST_CASE("shell command caps output and reports truncation") {
  const auto result = run_shell_command("printf 1234567890", ".", std::chrono::seconds(2), 5);
  REQUIRE(result.exit_code == 0);
  REQUIRE(result.output == "12345");
  REQUIRE(result.truncated);
}

TEST_CASE("shell command enforces a caller supplied timeout") {
  const auto result = run_shell_command("sleep 2", ".", std::chrono::milliseconds(50));
  REQUIRE(result.timed_out);
  REQUIRE(result.exit_code != 0);
}
