#include <catch2/catch_test_macros.hpp>

#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <string>

#include "interaction/git_commands.hpp"

namespace {

std::filesystem::path temp_repo() {
  const auto root = std::filesystem::temp_directory_path() /
                    ("booth-git-" + std::to_string(::getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  REQUIRE(std::system(("git -C " + root.string() + " init -q").c_str()) == 0);
  std::ofstream(root / "file.txt") << "one\n";
  return root;
}

struct Capture {
  booth::ControlHooks hooks;
  std::string text;
  bool failed{false};

  explicit Capture(booth::ControlClient* control, std::filesystem::path cwd) {
    hooks.control = control;
    hooks.cwd = cwd.string();
    hooks.publish = [this](booth::TranscriptEntry entry) {
      text = std::move(entry.text);
      failed = entry.status == "failed";
    };
    hooks.post = [](std::function<void()> job) { job(); };
  }
};

}  // namespace

TEST_CASE("git commands are scoped to the git and commit tokens") {
  REQUIRE(booth::is_git_command("/git"));
  REQUIRE(booth::is_git_command("/commit fix the build"));
  REQUIRE_FALSE(booth::is_git_command("/gitui"));
  REQUIRE_FALSE(booth::is_git_command("/status"));
}

TEST_CASE("git status and diff run without a shell") {
  const auto repo = temp_repo();
  Capture capture(nullptr, repo);

  REQUIRE(booth::handle_git_command(capture.hooks, "/git status"));
  REQUIRE_FALSE(capture.failed);
  REQUIRE(capture.text.find("?? file.txt") != std::string::npos);

  REQUIRE(booth::handle_git_command(capture.hooks, "/git diff"));
  REQUIRE_FALSE(capture.failed);
  REQUIRE(capture.text == "no diff");

  REQUIRE(booth::handle_git_command(capture.hooks, "/git stage file.txt"));
  REQUIRE_FALSE(capture.failed);
  REQUIRE(capture.text == "staged file.txt");

  REQUIRE(booth::handle_git_command(capture.hooks, "/git diff staged"));
  REQUIRE_FALSE(capture.failed);
  REQUIRE(capture.text.find("+one") != std::string::npos);

  std::ofstream(repo / "file.txt") << "one\ntwo\n";
  REQUIRE(std::system(("git -C " + repo.string() + " add file.txt").c_str()) == 0);
  REQUIRE(std::system(("git -C " + repo.string() +
                       " -c user.name=booth -c user.email=booth@example.test commit -q -m two")
                          .c_str()) == 0);
  REQUIRE(booth::handle_git_command(capture.hooks, "/git HEAD"));
  REQUIRE_FALSE(capture.failed);
  REQUIRE(capture.text.find("+two") != std::string::npos);

  std::filesystem::remove_all(repo);
}
