#include "interaction/ssh_config.hpp"

#include <algorithm>
#include <cctype>
#include <filesystem>
#include <fstream>
#include <map>
#include <optional>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using json = nlohmann::json;

constexpr std::string_view kAddUsage =
    "Usage: /ssh add <name> --host <host> [--user <user>] [--port <port>] [--key <keyPath>] "
    "[--scope project|user]";

std::string help_text() {
  return "SSH host management (ACP mode)\n"
         "  /ssh add <name> --host <host> [--user <user>] [--port <port>] [--key <keyPath>] "
         "[--scope project|user]\n"
         "  /ssh list                                       List configured SSH hosts\n"
         "  /ssh remove <name> [--scope project|user]       Remove an SSH host\n"
         "  /ssh help                                        Show this help";
}

std::vector<std::string> words(std::string_view text) {
  std::vector<std::string> out;
  std::string current;
  char quote = 0;
  for (const char ch : text) {
    if (quote != 0) {
      if (ch == quote) quote = 0;
      else current += ch;
      continue;
    }
    if (ch == '\'' || ch == '"') {
      quote = ch;
      continue;
    }
    if (std::isspace(static_cast<unsigned char>(ch)) != 0) {
      if (!current.empty()) {
        out.push_back(std::move(current));
        current.clear();
      }
      continue;
    }
    current += ch;
  }
  if (!current.empty()) out.push_back(std::move(current));
  return out;
}

bool valid_name(std::string_view name) {
  if (name.empty() || name.size() > 100) return false;
  return std::all_of(name.begin(), name.end(), [](unsigned char ch) {
    return std::isalnum(ch) != 0 || ch == '_' || ch == '.' || ch == '-';
  });
}

bool valid_port(std::string_view text, int& port) {
  if (text.empty() || !std::all_of(text.begin(), text.end(),
                                   [](unsigned char ch) { return std::isdigit(ch) != 0; })) {
    return false;
  }
  unsigned long value = 0;
  for (const char ch : text) {
    value = value * 10 + static_cast<unsigned long>(ch - '0');
    if (value > 65535) return false;
  }
  if (value < 1) return false;
  port = static_cast<int>(value);
  return true;
}

bool read_config(const std::filesystem::path& path, json& document, std::string& error) {
  document = json::object();
  document["hosts"] = json::object();
  std::error_code ec;
  if (!std::filesystem::exists(path, ec)) return true;
  std::ifstream input(path);
  if (!input.is_open()) {
    error = "cannot read " + path.string();
    return false;
  }
  document = json::parse(input, nullptr, false);
  if (document.is_discarded() || !document.is_object()) {
    error = "invalid SSH config " + path.string();
    return false;
  }
  if (!document.contains("hosts") || !document["hosts"].is_object()) document["hosts"] = json::object();
  return true;
}

bool write_config(const std::filesystem::path& path, const json& document, std::string& error) {
  std::error_code ec;
  if (!path.parent_path().empty()) std::filesystem::create_directories(path.parent_path(), ec);
  if (ec) {
    error = ec.message();
    return false;
  }
  auto temp = path;
  temp += ".tmp";
  {
    std::ofstream output(temp, std::ios::out | std::ios::trunc | std::ios::binary);
    if (!output.is_open()) {
      error = "cannot write " + temp.string();
      return false;
    }
    output << document.dump(2) << '\n';
  }
  std::filesystem::permissions(temp, std::filesystem::perms::owner_read | std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, ec);
  std::filesystem::rename(temp, path, ec);
  if (ec) {
    error = "cannot replace " + path.string() + ": " + ec.message();
    return false;
  }
  std::filesystem::permissions(path, std::filesystem::perms::owner_read | std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, ec);
  return true;
}

std::string host_line(const std::string& name, const json& host, std::string_view scope) {
  const std::string address = host.value("host", std::string{"?"});
  const std::string user = host.value("username", std::string{"-"});
  const int port = host.value("port", 0);
  return name + " | " + address + " | " + user + " | " + std::to_string(port == 0 ? 22 : port) +
         " [" + std::string(scope) + "]";
}

std::string list_hosts(const std::filesystem::path& project_path, const std::filesystem::path& user_path) {
  json project;
  json user;
  std::string error;
  if (!read_config(project_path, project, error) || !read_config(user_path, user, error)) return error;
  std::string out;
  std::vector<std::string> seen;
  for (const auto& [name, host] : project["hosts"].items()) {
    if (!host.is_object()) continue;
    if (!out.empty()) out += '\n';
    out += host_line(name, host, "project");
    seen.push_back(name);
  }
  for (const auto& [name, host] : user["hosts"].items()) {
    if (!host.is_object()) continue;
    if (std::find(seen.begin(), seen.end(), name) != seen.end()) continue;
    if (!out.empty()) out += '\n';
    out += host_line(name, host, "user");
  }
  return out.empty() ? "No SSH hosts configured." : out;
}

struct AddOptions {
  std::string name;
  std::string host;
  std::string username;
  std::string key_path;
  int port{0};
  std::string scope{"project"};
};

bool parse_add(const std::vector<std::string>& args, AddOptions& options, std::string& error) {
  if (args.empty()) {
    error = kAddUsage;
    return false;
  }
  std::size_t index = 0;
  if (!args[0].empty() && args[0][0] != '-') {
    options.name = args[0];
    index = 1;
  }
  while (index < args.size()) {
    const std::string& flag = args[index];
    if (flag != "--host" && flag != "--user" && flag != "--port" && flag != "--key" &&
       flag != "--scope") {
      error = "Unknown option: " + flag;
      return false;
    }
    if (index + 1 >= args.size()) {
      error = "Missing value for " + flag + ".";
      return false;
    }
    const std::string& value = args[index + 1];
    if (flag == "--host") options.host = value;
    else if (flag == "--user") options.username = value;
    else if (flag == "--key") options.key_path = value;
    else if (flag == "--port") {
      if (!valid_port(value, options.port)) {
        error = "Invalid --port value. Must be an integer between 1 and 65535.";
        return false;
      }
    } else if (flag == "--scope") {
      if (value != "project" && value != "user") {
        error = "Invalid --scope value. Use project or user.";
        return false;
      }
      options.scope = value;
    }
    index += 2;
  }
  if (options.name.empty()) {
    error = "Host name required. " + std::string(kAddUsage);
    return false;
  }
  if (!valid_name(options.name)) {
    error = "Host name can only contain letters, numbers, dash, underscore, and dot (max 100 characters)";
    return false;
  }
  if (options.host.empty()) {
    error = "--host is required. " + std::string(kAddUsage);
    return false;
  }
  return true;
}

}  // namespace

std::string handle_ssh_command(std::string_view args, const std::filesystem::path& user_config,
                               const std::filesystem::path& project_config) {
  const auto tokens = words(args);
  if (tokens.empty() || tokens[0] == "help") return help_text();
  const std::string verb = tokens[0];
  if (verb == "list") return list_hosts(project_config, user_config);
  if (verb == "add") {
    AddOptions options;
    std::string error;
    if (!parse_add(std::vector<std::string>(tokens.begin() + 1, tokens.end()), options, error)) {
      return error;
    }
    const auto& path = options.scope == "user" ? user_config : project_config;
    json document;
    if (!read_config(path, document, error)) return error;
    if (document["hosts"].contains(options.name)) {
      return "Host \"" + options.name + "\" already exists in " + path.string();
    }
    json host = {{"host", options.host}};
    if (!options.username.empty()) host["username"] = options.username;
    if (options.port != 0) host["port"] = options.port;
    if (!options.key_path.empty()) host["keyPath"] = options.key_path;
    document["hosts"][options.name] = std::move(host);
    if (!write_config(path, document, error)) return error;
    return "Added SSH host \"" + options.name + "\" (" + options.scope + ").";
  }
  if (verb == "remove" || verb == "rm") {
    std::string name;
    std::string scope = "project";
    std::size_t index = 1;
    if (index < tokens.size() && tokens[index] != "--scope") name = tokens[index++];
    while (index < tokens.size()) {
      if (tokens[index] != "--scope" || index + 1 >= tokens.size()) {
        return "Usage: /ssh remove <name> [--scope project|user]";
      }
      scope = tokens[index + 1];
      if (scope != "project" && scope != "user") {
        return "Invalid --scope value. Use project or user.";
      }
      index += 2;
    }
    if (name.empty()) return "Usage: /ssh remove <name> [--scope project|user]";
    const auto& path = scope == "user" ? user_config : project_config;
    json document;
    std::string error;
    if (!read_config(path, document, error)) return error;
    if (!document["hosts"].contains(name)) {
      return "Host \"" + name + "\" not found in " + path.string();
    }
    document["hosts"].erase(name);
    if (!write_config(path, document, error)) return error;
    return "Removed SSH host \"" + name + "\" from " + scope + " config.";
  }
  return "Unknown /ssh subcommand: " + verb + ". Use /ssh help for available subcommands.";
}

}  // namespace booth
