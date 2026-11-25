#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

#include "interaction/file_command.hpp"

using namespace booth;

namespace {
std::filesystem::path command_dir() {
  return std::filesystem::temp_directory_path() /
         ("booth-commands-" + std::to_string(::getpid()));
}

std::filesystem::path write_command(const std::string& name, const std::string& body) {
  std::filesystem::create_directories(command_dir());
  const auto path = command_dir() / (name + ".md");
  std::ofstream(path, std::ios::trunc) << body;
  return path;
}
}  // namespace

TEST_CASE("file commands read frontmatter and fall back to the first body line") {
  const auto path = write_command("review", "---\ndescription: review a diff\n---\nReview $ARGUMENTS carefully.\n");
  const auto command = read_file_command(path);
  REQUIRE(command.has_value());
  REQUIRE(command->name == "review");
  REQUIRE(command->description == "review a diff");
  REQUIRE(command->body == "Review $ARGUMENTS carefully.");
  std::filesystem::remove(path);

  const auto fallback = write_command("fix", "Fix the failing test.\nSecond line.\n");
  const auto simple = read_file_command(fallback);
  REQUIRE(simple.has_value());
  REQUIRE(simple->description == "Fix the failing test.");
  std::filesystem::remove(fallback);
  std::filesystem::remove_all(command_dir());
}

TEST_CASE("file commands expand positional and aggregate arguments") {
  FileCommand command{"review", "", "Check $1 against $2.\n$ARGUMENTS"};
  REQUIRE(expand_file_command(command, "alpha beta") == "Check alpha against beta.\nalpha beta");

  FileCommand append{"fix", "", "Fix the build."};
  REQUIRE(expand_file_command(append, "src/a.cpp") == "Fix the build.\nsrc/a.cpp");
  REQUIRE(expand_file_command(append, "") == "Fix the build.");
}
