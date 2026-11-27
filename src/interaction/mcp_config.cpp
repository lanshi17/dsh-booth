#include "interaction/mcp_config.hpp"

#include <fstream>
#include <string>
#include <utility>
#include <vector>

namespace booth {
namespace {

using json = nlohmann::json;

json read_document(const std::filesystem::path& file) {
  std::ifstream input(file);
  if (!input) return json{{"servers", json::array()}};
  auto document = json::parse(input, nullptr, false);
  if (document.is_discarded() || !document.is_object()) return json{{"servers", json::array()}};
  if (!document.contains("servers") || !document["servers"].is_array()) {
    document["servers"] = json::array();
  }
  return document;
}

bool write_document(const std::filesystem::path& file, const json& document) {
  std::error_code error;
  std::filesystem::create_directories(file.parent_path(), error);
  auto temporary = file;
  temporary += ".tmp";
  std::ofstream output(temporary, std::ios::trunc);
  if (!output) return false;
  output << document.dump(2) << '\n';
  output.close();
  if (!output) return false;
  std::filesystem::rename(temporary, file, error);
  if (error) {
    std::filesystem::remove(temporary, error);
    return false;
  }
  std::filesystem::permissions(file,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
  return true;
}

}  // namespace

json load_mcp_servers(const std::filesystem::path& file) {
  const auto document = read_document(file);
  json servers = json::array();
  for (const auto& server : document["servers"]) {
    if (!server.is_object()) continue;
    if (server.value("enabled", true) == false) continue;
    json row = server;
    row.erase("enabled");
    servers.push_back(std::move(row));
  }
  return servers;
}

json upsert_mcp_server(const std::filesystem::path& file, std::string name, json server) {
  auto document = read_document(file);
  server["name"] = name;
  bool replaced = false;
  for (auto& existing : document["servers"]) {
    if (existing.is_object() && existing.value("name", std::string{}) == name) {
      existing = server;
      replaced = true;
      break;
    }
  }
  if (!replaced) document["servers"].push_back(server);
  if (!write_document(file, document)) return json{};
  return server;
}

bool remove_mcp_server(const std::filesystem::path& file, const std::string& name) {
  auto document = read_document(file);
  const auto before = document["servers"].size();
  json kept = json::array();
  for (const auto& server : document["servers"]) {
    if (server.is_object() && server.value("name", std::string{}) == name) continue;
    kept.push_back(server);
  }
  if (kept.size() == before) return false;
  document["servers"] = std::move(kept);
  return write_document(file, document);
}

bool set_mcp_enabled(const std::filesystem::path& file, const std::string& name, bool enabled) {
  auto document = read_document(file);
  for (auto& server : document["servers"]) {
    if (!server.is_object() || server.value("name", std::string{}) != name) continue;
    server["enabled"] = enabled;
    return write_document(file, document);
  }
  return false;
}

std::string mcp_test_text(const std::filesystem::path& file, const std::string& name) {
  auto document = read_document(file);
  for (const auto& server : document["servers"]) {
    if (!server.is_object() || server.value("name", std::string{}) != name) continue;
    const bool enabled = server.value("enabled", true);
    std::string out = "mcp " + name + " · " + (enabled ? "enabled" : "disabled") + " · ";
    if (server.contains("url") && server["url"].is_string()) {
      const std::string url = server["url"].get<std::string>();
      if (url.rfind("http://", 0) != 0 && url.rfind("https://", 0) != 0) {
        return "mcp " + name + " · invalid url";
      }
      return out + "http " + url + " · dsh connects on the next session";
    }
    if (server.contains("command") && server["command"].is_string() &&
        !server["command"].get<std::string>().empty()) {
      return out + "stdio " + server["command"].get<std::string>() +
             " · dsh connects on the next session";
    }
    return "mcp " + name + " · missing command or url";
  }
  return "no mcp server " + name;
}

std::string mcp_list_text(const std::filesystem::path& file) {
  const auto document = read_document(file);
  const auto& servers = document["servers"];
  if (servers.empty() || !servers.is_array()) return "no MCP servers · /mcp add <name> <command|url>";
  std::string out = "MCP servers";
  for (const auto& server : servers) {
    if (!server.is_object()) continue;
    out += " · " + server.value("name", std::string{"server"});
    if (server.contains("url")) out += " " + server.value("url", std::string{});
    else out += " " + server.value("command", std::string{});
    if (server.value("enabled", true) == false) out += " (disabled)";
  }
  return out;
}

}  // namespace booth
