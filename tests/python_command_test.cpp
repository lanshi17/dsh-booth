#include <catch2/catch_test_macros.hpp>

#include <string>

#include "interaction/shell_command.hpp"

using namespace booth;

TEST_CASE("python command evaluates a trailing expression") {
  const auto result = run_python_command("2 + 2", "/");
  REQUIRE(result.exit_code == 0);
  REQUIRE(result.output == "4\n");
}

TEST_CASE("python command executes statements and captures stderr") {
  auto result = run_python_command("print('hello')\n", "/");
  REQUIRE(result.exit_code == 0);
  REQUIRE(result.output == "hello\n");
  result = run_python_command("raise SystemExit(3)", "/");
  REQUIRE(result.exit_code == 3);
  result = run_python_command("def (", "/");
  REQUIRE(result.exit_code != 0);
  REQUIRE(result.output.find("SyntaxError") != std::string::npos);
}

TEST_CASE("python command survives quotes and unicode") {
  const auto result = run_python_command("print(\"it's ✓\")", "/");
  REQUIRE(result.exit_code == 0);
  REQUIRE(result.output == "it's ✓\n");
}
