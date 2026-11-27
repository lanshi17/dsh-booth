#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <string>

#include <unistd.h>

#include "interaction/export.hpp"
#include "interaction/state.hpp"

namespace {

std::filesystem::path temp_dir() {
  const auto root = std::filesystem::temp_directory_path() /
                    ("booth-export-" + std::to_string(::getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  return root;
}

std::string read_file(const std::filesystem::path& path) {
  std::ifstream input(path);
  return {std::istreambuf_iterator<char>(input), std::istreambuf_iterator<char>()};
}

}  // namespace

TEST_CASE("HTML export escapes transcript text and is self-contained") {
  booth::AppState state;
  state.session = "s-1";
  state.title = "a <title>";
  state.model = "model";
  booth::TranscriptEntry user;
  user.kind = booth::EntryKind::User;
  user.text = "<script>alert(1)</script>";
  state.entries.push_back(user);
  booth::TranscriptEntry tool;
  tool.kind = booth::EntryKind::Tool;
  tool.tool = "bash";
  tool.input = "\"a&b\"";
  tool.output = "<unsafe>";
  state.entries.push_back(tool);

  const std::string html = booth::transcript_html(state);
  REQUIRE(html.find("<!doctype html>") == 0);
  REQUIRE(html.find("&lt;script&gt;alert(1)&lt;/script&gt;") != std::string::npos);
  REQUIRE(html.find("<script>alert(1)</script>") == std::string::npos);
  REQUIRE(html.find("&lt;unsafe&gt;") != std::string::npos);
  REQUIRE(html.find("a &lt;title&gt;") != std::string::npos);
}

TEST_CASE("export defaults to HTML and keeps Markdown for .md paths") {
  const auto root = temp_dir();
  booth::AppState state;
  booth::TranscriptEntry user;
  user.kind = booth::EntryKind::User;
  user.text = "hello";
  state.entries.push_back(user);

  booth::export_command(state, "/export", root, root.string());
  REQUIRE(state.notice.text.rfind("exported 1 entries to ", 0) == 0);
  const auto default_path = std::filesystem::path(state.notice.text.substr(std::string("exported 1 entries to ").size()));
  REQUIRE(default_path.extension() == ".html");
  REQUIRE(read_file(default_path).find("<!doctype html>") == 0);

  booth::export_command(state, "/export notes.md", root, root.string());
  const auto markdown_path = root / "notes.md";
  REQUIRE(read_file(markdown_path).find("# booth transcript") == 0);

  booth::export_command(state, "/export --copy", root, root.string());
  REQUIRE(state.notice.text == "use /dump to copy the session to the clipboard");
  std::filesystem::remove_all(root);
}

TEST_CASE("explicit HTML path and themes flag render HTML") {
  const auto root = temp_dir();
  booth::AppState state;
  booth::TranscriptEntry user;
  user.kind = booth::EntryKind::User;
  user.text = "hello";
  state.entries.push_back(user);

  booth::export_command(state, "/export page.html", root, root.string());
  REQUIRE(read_file(root / "page.html").find("<!doctype html>") == 0);

  booth::export_command(state, "/export --themes themed", root, root.string());
  REQUIRE(read_file(root / "themed.html").find("<!doctype html>") == 0);

  booth::export_command(state, "/export --bad", root, root.string());
  REQUIRE(state.notice.text == "usage: /export [--themes] [path]");
  std::filesystem::remove_all(root);
}
