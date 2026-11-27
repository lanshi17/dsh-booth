#include "interaction/ssh_config.hpp"

#include <filesystem>
#include <fstream>
#include <string>

#include <catch2/catch_test_macros.hpp>
#include <nlohmann/json.hpp>

namespace {

std::filesystem::path temp_dir(const std::string& name) {
  const auto dir = std::filesystem::temp_directory_path() / name;
  std::filesystem::remove_all(dir);
  std::filesystem::create_directories(dir);
  return dir;
}

}  // namespace

TEST_CASE("ssh adds lists and removes scoped hosts") {
  const auto root = temp_dir("booth-ssh-test");
  const auto user = root / "user" / "ssh.json";
  const auto project = root / "project" / ".booth" / "ssh.json";

  auto result = booth::handle_ssh_command(
      "add box --host 10.0.0.1 --user root --port 2222 --key /tmp/key", user, project);
  REQUIRE(result.find("Added SSH host \"box\" (project).") != std::string::npos);

  std::ifstream input(project);
  const auto document = nlohmann::json::parse(input);
  REQUIRE(document["hosts"]["box"]["host"] == "10.0.0.1");
  REQUIRE(document["hosts"]["box"]["username"] == "root");
  REQUIRE(document["hosts"]["box"]["port"] == 2222);
  REQUIRE(document["hosts"]["box"]["keyPath"] == "/tmp/key");

  result = booth::handle_ssh_command("list", user, project);
  REQUIRE(result.find("box | 10.0.0.1 | root | 2222 [project]") != std::string::npos);

  result = booth::handle_ssh_command("add box --host 10.0.0.2", user, project);
  REQUIRE(result.find("already exists") != std::string::npos);

  result = booth::handle_ssh_command("add box --host 10.0.0.3 --scope user", user, project);
  REQUIRE(result.find("(user)") != std::string::npos);
  result = booth::handle_ssh_command("list", user, project);
  REQUIRE(result.find("[project]") != std::string::npos);
  REQUIRE(result.find("[user]") == std::string::npos);

  result = booth::handle_ssh_command("remove box", user, project);
  REQUIRE(result.find("Removed SSH host") != std::string::npos);
  result = booth::handle_ssh_command("list", user, project);
  REQUIRE(result.find("[user]") != std::string::npos);

  REQUIRE(booth::handle_ssh_command("add bad --host x --port 0", user, project).find("Invalid --port") !=
          std::string::npos);
  REQUIRE(booth::handle_ssh_command("add bad --host x --scope nope", user, project).find("Invalid --scope") !=
          std::string::npos);
  std::filesystem::remove_all(root);
}
