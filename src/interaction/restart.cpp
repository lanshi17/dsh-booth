#include "interaction/restart.hpp"

#include <unistd.h>

#include <cerrno>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <string>
#include <vector>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

using json = nlohmann::json;

}  // namespace

bool write_restart_marker(const std::filesystem::path& file, const RestartMarker& marker,
                          std::string& error) {
  std::error_code code;
  std::filesystem::create_directories(file.parent_path(), code);
  auto temporary = file;
  temporary += ".tmp";
  std::ofstream output(temporary, std::ios::trunc);
  if (!output) {
    error = "could not write " + temporary.string();
    return false;
  }
  output << json{{"session", marker.session}, {"cwd", marker.cwd}, {"title", marker.title}}.dump(2)
         << '\n';
  output.close();
  if (!output) {
    error = "could not write " + temporary.string();
    return false;
  }
  std::filesystem::rename(temporary, file, code);
  if (code) {
    error = "could not replace " + file.string();
    std::filesystem::remove(temporary, code);
    return false;
  }
  std::filesystem::permissions(file,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, code);
  return true;
}

std::optional<RestartMarker> take_restart_marker(const std::filesystem::path& file) {
  std::ifstream input(file, std::ios::binary);
  if (!input) return std::nullopt;
  const json document = json::parse(input, nullptr, false);
  input.close();
  std::error_code code;
  std::filesystem::remove(file, code);
  if (document.is_discarded() || !document.is_object()) return std::nullopt;
  const std::string session = document.value("session", std::string{});
  if (session.empty()) return std::nullopt;
  return RestartMarker{session, document.value("cwd", std::string{}),
                       document.value("title", std::string{})};
}

void exec_restart() {
  std::ifstream input("/proc/self/cmdline", std::ios::binary);
  std::vector<std::string> storage;
  if (input) {
    std::string value;
    char ch = 0;
    while (input.get(ch)) {
      if (ch == '\0') {
        storage.push_back(std::move(value));
        value.clear();
      } else {
        value += ch;
      }
    }
    if (!value.empty()) storage.push_back(std::move(value));
  }
  if (storage.empty()) storage.push_back("/proc/self/exe");
  std::vector<char*> argv;
  argv.reserve(storage.size() + 1);
  for (auto& value : storage) argv.push_back(value.data());
  argv.push_back(nullptr);
  ::execv("/proc/self/exe", argv.data());
  std::fprintf(stderr, "booth: cannot restart: %s\n", std::strerror(errno));
}

}  // namespace booth
