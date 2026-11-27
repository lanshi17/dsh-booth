#include <catch2/catch_test_macros.hpp>

#include <filesystem>
#include <fstream>

#include "interaction/mcp_config.hpp"

namespace {

std::filesystem::path temp_file() {
  const auto dir = std::filesystem::temp_directory_path() / "booth-mcp-test";
  std::filesystem::create_directories(dir);
  std::filesystem::remove(dir / "mcp.json");
  return dir / "mcp.json";
}

}  // namespace

TEST_CASE("mcp config upserts, lists and removes servers") {
  const auto file = temp_file();
  REQUIRE(booth::load_mcp_servers(file).empty());
  REQUIRE(booth::mcp_list_text(file).find("no MCP servers") != std::string::npos);

  auto saved = booth::upsert_mcp_server(
      file, "files", nlohmann::json{{"command", "npx"}, {"args", {"-y", "server-files"}}});
  REQUIRE(saved["name"] == "files");
  auto remote = booth::upsert_mcp_server(file, "remote", nlohmann::json{{"url", "https://mcp.example/sse"}});
  REQUIRE(remote["name"] == "remote");

  auto servers = booth::load_mcp_servers(file);
  REQUIRE(servers.size() == 2);
  REQUIRE(servers[0]["command"] == "npx");
  REQUIRE(servers[0]["args"][1] == "server-files");
  REQUIRE(servers[1]["url"] == "https://mcp.example/sse");
  REQUIRE(booth::mcp_list_text(file).find("remote") != std::string::npos);

  saved = booth::upsert_mcp_server(file, "files", nlohmann::json{{"command", "uvx"}});
  REQUIRE(booth::load_mcp_servers(file).size() == 2);
  REQUIRE(booth::load_mcp_servers(file)[0]["command"] == "uvx");

  REQUIRE(booth::remove_mcp_server(file, "files"));
  REQUIRE_FALSE(booth::remove_mcp_server(file, "files"));
  REQUIRE(booth::load_mcp_servers(file).size() == 1);
}

TEST_CASE("mcp config tolerates a malformed document") {
  const auto file = temp_file();
  {
    std::ofstream output(file);
    output << "{not json\n";
  }
  REQUIRE(booth::load_mcp_servers(file).empty());
  REQUIRE(booth::upsert_mcp_server(file, "one", nlohmann::json{{"command", "run"}})["name"] == "one");
  REQUIRE(booth::load_mcp_servers(file).size() == 1);
}
