#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>
#include <optional>

#include <unistd.h>
#include <string>

#include <nlohmann/json.hpp>

#include "interaction/extra_commands.hpp"
#include "interaction/mcp_config.hpp"

namespace {

std::filesystem::path temp_state() {
  const auto root = std::filesystem::temp_directory_path() /
                    ("booth-extra-" + std::to_string(::getpid()));
  std::filesystem::remove_all(root);
  std::filesystem::create_directories(root);
  return root;
}

}  // namespace

TEST_CASE("dump writes an anonymized transcript") {
  const auto state_dir = temp_state();
  booth::AppState state;
  state.session = "s-1";
  state.title = "secret title";
  state.model = "model";
  booth::TranscriptEntry entry;
  entry.kind = booth::EntryKind::Assistant;
  entry.text = "secret answer";
  state.entries.push_back(entry);

  REQUIRE(booth::handle_extra_command(state, "/dump anon", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("dumped transcript to ") == 0);
  const auto path = std::filesystem::path(std::string(state.notice).substr(std::string("dumped transcript to ").size()));
  REQUIRE(std::filesystem::exists(path));
  std::ifstream input(path);
  const auto document = nlohmann::json::parse(input);
  REQUIRE(document["anonymized"] == true);
  REQUIRE(document["session"] == "<redacted>");
  REQUIRE(document["entries"][0]["text"] == "<redacted>");
  std::filesystem::remove(path);
  std::filesystem::remove_all(state_dir);
}

TEST_CASE("debug and extensions report real state and profile metadata") {
  const auto state_dir = temp_state();
  booth::AppState state;
  state.session = "s-debug";
  state.model = "model-x";
  state.reasoning.current_name = "High";
  booth::TranscriptEntry tool;
  tool.kind = booth::EntryKind::Tool;
  state.entries.push_back(tool);
  REQUIRE(booth::handle_extra_command(state, "/debug", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("session s-debug") != std::string::npos);
  REQUIRE(state.notice.find("model model-x") != std::string::npos);
  REQUIRE(state.notice.find("effort High") != std::string::npos);
  REQUIRE(state.notice.find("tools 1") != std::string::npos);

  const auto home = temp_state() / "dsh";
  const auto profile = home / "profiles" / "probe";
  std::filesystem::create_directories(profile);
  std::ofstream(profile / "package.json")
      << R"({"dependencies":{"one":"1"},"dsh":{"profile":{"bundles":["base","acp"]}}})";
  std::ofstream(profile / "cordis.patch.yml")
      << "- id: one\n- id: two\n- insert:\n    - id: nested\n";
  const char* old_home = ::getenv("DSH_HOME");
  const char* old_base = ::getenv("BOOTH_BASE_PROFILE");
  const std::string saved_home = old_home == nullptr ? "" : old_home;
  const std::string saved_base = old_base == nullptr ? "" : old_base;
  ::setenv("DSH_HOME", home.c_str(), 1);
  ::setenv("BOOTH_BASE_PROFILE", "probe", 1);
  REQUIRE(booth::handle_extra_command(state, "/extensions", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("extensions probe") != std::string::npos);
  REQUIRE(state.notice.find("bundles 2") != std::string::npos);
  REQUIRE(state.notice.find("dependencies 1") != std::string::npos);
  REQUIRE(state.notice.find("patch rows 3") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/plugins", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("one 1") != std::string::npos);
  REQUIRE(state.notice.find("base") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/plugins disable one", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("managed by package.json") != std::string::npos);
  if (old_home == nullptr) ::unsetenv("DSH_HOME"); else ::setenv("DSH_HOME", saved_home.c_str(), 1);
  if (old_base == nullptr) ::unsetenv("BOOTH_BASE_PROFILE"); else ::setenv("BOOTH_BASE_PROFILE", saved_base.c_str(), 1);
  std::filesystem::remove_all(state_dir);
  std::filesystem::remove_all(home);
}

TEST_CASE("changelog reads the configured file and validates the view") {
  const auto state_dir = temp_state();
  const auto path = state_dir / "CHANGELOG.md";
  std::ofstream(path) << "# Changelog\n\n## [9.9.9] - today\n- nine\n\n## [8.0.0] - earlier\n- eight\n";
  const char* old = ::getenv("BOOTH_CHANGELOG");
  const std::string saved = old == nullptr ? "" : old;
  ::setenv("BOOTH_CHANGELOG", path.c_str(), 1);
  booth::AppState state;
  REQUIRE(booth::handle_extra_command(state, "/changelog last 1", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("9.9.9") != std::string::npos);
  REQUIRE(state.notice.find("8.0.0") == std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/changelog full", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("8.0.0") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/changelog last 0", state_dir.string(), state_dir));
  REQUIRE(state.notice.text.rfind("Usage", 0) == 0);
  if (old == nullptr) ::unsetenv("BOOTH_CHANGELOG"); else ::setenv("BOOTH_CHANGELOG", saved.c_str(), 1);
  std::filesystem::remove_all(state_dir);
}

TEST_CASE("local lists state files and mcp persists servers") {
  const auto state_dir = temp_state();
  std::ofstream(state_dir / "theme.conf") << "accent=#fff\n";
  booth::AppState state;

  REQUIRE(booth::handle_extra_command(state, "/local", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("theme.conf") != std::string::npos);

  REQUIRE(booth::handle_extra_command(state, "/mcp add files npx -y server-files", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("mcp files saved") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp list", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("files") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp remove files", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("removed") != std::string::npos);
  std::filesystem::remove_all(state_dir);
}

TEST_CASE("unavailable OMP commands report why") {
  const auto state_dir = temp_state();
  booth::AppState state;
  REQUIRE(booth::handle_extra_command(state, "/live", state_dir.string(), state_dir));
  REQUIRE(state.notice == "live voice is unavailable: dsh exposes no audio transport");
  REQUIRE(booth::handle_extra_command(state, "/ratchet tighten", state_dir.string(), state_dir));
  REQUIRE(state.notice == "ratchet is unavailable: dsh exposes no eval kernel");
  std::filesystem::remove_all(state_dir);
}

TEST_CASE("mcp servers persist disabled state and test the transport") {
  const auto state_dir = temp_state();
  booth::AppState state;
  REQUIRE(booth::handle_extra_command(state, "/mcp add local echo hello", state_dir.string(), state_dir));
  REQUIRE(booth::handle_extra_command(state, "/mcp add remote --url https://example.test/mcp", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("remote saved") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp add bad --nope", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("unknown /mcp add option") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp add scoped --scope team echo", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("usage: /mcp add") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp test local", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("stdio echo") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp disable local", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("disabled") != std::string::npos);
  const auto servers = booth::load_mcp_servers(state_dir / "mcp.json");
  REQUIRE(servers.size() == 1);
  REQUIRE(servers[0]["name"] == "remote");
  REQUIRE(booth::handle_extra_command(state, "/mcp list", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("(disabled)") != std::string::npos);
  REQUIRE(booth::handle_extra_command(state, "/mcp enable local", state_dir.string(), state_dir));
  REQUIRE(booth::load_mcp_servers(state_dir / "mcp.json").size() == 2);
  REQUIRE(booth::handle_extra_command(state, "/mcp reload", state_dir.string(), state_dir));
  REQUIRE(state.notice.find("/new") != std::string::npos);
  std::filesystem::remove_all(state_dir);
}

TEST_CASE("open validates its optional link argument") {
  const auto state_dir = temp_state();
  booth::AppState state;
  REQUIRE(booth::handle_extra_command(state, "/open nope", state_dir.string(), state_dir));
  REQUIRE(state.notice == "usage: /open [link]");
  REQUIRE(booth::handle_extra_command(state, "/open link", state_dir.string(), state_dir));
  REQUIRE(state.notice == "no links in the transcript");
  std::filesystem::remove_all(state_dir);
}
