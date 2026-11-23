#include <catch2/catch_test_macros.hpp>

#include <unistd.h>

#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/file_menu.hpp"

using namespace booth;

namespace {

std::filesystem::path make_root(std::string_view name) {
  const auto dir = std::filesystem::temp_directory_path() /
                   ("booth-files-" + std::string(name) + "-" + std::to_string(::getpid()));
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir / "dir");
  std::filesystem::create_directories(dir / "build");
  { std::ofstream out(dir / "alpha.txt"); out << "a"; }
  { std::ofstream out(dir / "dir" / "beta.md"); out << "b"; }
  { std::ofstream out(dir / "build" / "ignored.txt"); out << "i"; }
  return dir;
}

}  // namespace

TEST_CASE("file completion walks the tree but skips build directories") {
  const auto root = make_root("walk");
  const auto files = complete_files(root.string(), "");
  REQUIRE(files.size() == 2);
  REQUIRE(files[0] == "alpha.txt");
  REQUIRE(files[1] == "dir/beta.md");
  const auto filtered = complete_files(root.string(), "beta");
  REQUIRE(filtered.size() == 1);
  REQUIRE(filtered[0] == "dir/beta.md");
  std::filesystem::remove_all(root);
}

TEST_CASE("the inline file menu completes the token") {
  const auto root = make_root("menu");
  FileMenu menu;
  std::string composer = "see @bet";
  menu.sync(composer, root.string());
  REQUIRE(menu.visible());
  REQUIRE(menu.matches().size() == 1);
  ftxui::Event tab = ftxui::Event::Tab;
  REQUIRE(menu.key(tab, composer));
  REQUIRE(composer == "see @dir/beta.md ");
  REQUIRE_FALSE(menu.visible());
  std::filesystem::remove_all(root);
}

TEST_CASE("@ mentions become existing file resource links") {
  const auto root = make_root("links");
  const auto links = file_links("compare @alpha.txt with @missing.txt and @dir/beta.md", root.string());
  REQUIRE(links.size() == 2);
  REQUIRE(links[0].first == "alpha.txt");
  REQUIRE(links[0].second.rfind("file://", 0) == 0);
  REQUIRE(links[1].first == "beta.md");
  std::filesystem::remove_all(root);
}
