#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/draft.hpp"

using namespace booth;

namespace {

std::filesystem::path temp_path(std::string_view name) {
  return std::filesystem::temp_directory_path() /
         ("booth-draft-" + std::string(name) + "-" + std::to_string(::getpid()));
}

}  // namespace

TEST_CASE("a draft round-trips and empty text removes it") {
  const auto path = temp_path("roundtrip");
  std::filesystem::remove_all(path);

  REQUIRE(load_draft(path).empty());
  save_draft(path, "unsent prompt\nsecond line");
  REQUIRE(load_draft(path) == "unsent prompt\nsecond line");

  const auto status = std::filesystem::status(path);
  REQUIRE((status.permissions() & std::filesystem::perms::owner_read) !=
          std::filesystem::perms::none);

  save_draft(path, "");
  REQUIRE_FALSE(std::filesystem::exists(path));
}

TEST_CASE("a draft is replaced atomically") {
  const auto path = temp_path("replace");
  std::filesystem::remove_all(path);
  save_draft(path, "first");
  save_draft(path, "second");
  REQUIRE(load_draft(path) == "second");
  REQUIRE_FALSE(std::filesystem::exists(path.string() + ".tmp"));
  std::filesystem::remove_all(path);
}
