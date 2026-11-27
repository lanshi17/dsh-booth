#include "interaction/provider_launch.hpp"

#include <sys/stat.h>
#include <sys/wait.h>
#include <unistd.h>

#include <cctype>
#include <cerrno>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <string>
#include <string_view>
#include <vector>

namespace booth {
namespace {

std::string trim(std::string_view text) {
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.front())) != 0) text.remove_prefix(1);
  while (!text.empty() && std::isspace(static_cast<unsigned char>(text.back())) != 0) text.remove_suffix(1);
  return std::string(text);
}

bool executable(const std::filesystem::path& path) {
  struct stat info {};
  return !path.empty() && ::stat(path.c_str(), &info) == 0 && S_ISREG(info.st_mode) &&
         ::access(path.c_str(), X_OK) == 0;
}

std::filesystem::path sibling_binary() {
  std::error_code error;
  const auto self = std::filesystem::read_symlink("/proc/self/exe", error);
  if (error || self.empty()) return {};
  return self.parent_path() / "booth-provider";
}

std::filesystem::path find_binary() {
  if (const char* configured = ::getenv("BOOTH_PROVIDER_BIN");
      configured != nullptr && *configured != '\0') {
    std::filesystem::path path(configured);
    if (executable(path)) return path;
    return {};
  }
  if (const auto sibling = sibling_binary(); executable(sibling)) return sibling;
  if (const char* path = ::getenv("PATH"); path != nullptr) {
    std::string remaining(path);
    for (;;) {
      const auto colon = remaining.find(':');
      const std::string dir = remaining.substr(0, colon);
      if (!dir.empty()) {
        const auto candidate = std::filesystem::path(dir) / "booth-provider";
        if (executable(candidate)) return candidate;
      }
      if (colon == std::string::npos) break;
      remaining.erase(0, colon + 1);
    }
  }
  return {};
}

}  // namespace

bool is_provider_command(std::string_view line) {
  if (line == "/providers") return true;
  return line.size() > 10 && line.rfind("/providers", 0) == 0 &&
         std::isspace(static_cast<unsigned char>(line[10])) != 0;
}

std::string provider_command_argument(std::string_view line) {
  return is_provider_command(line) && line.size() > 10 ? trim(line.substr(10)) : std::string{};
}

bool handle_provider_command(std::string_view line, ftxui::ScreenInteractive& screen,
                             std::string& notice) {
  if (!is_provider_command(line)) return false;
  notice = launch_provider_editor(screen, provider_command_argument(line));
  return true;
}

std::string launch_provider_editor(ftxui::ScreenInteractive& screen, const std::string& argument) {
  const auto binary = find_binary();
  if (binary.empty()) {
    return "booth-provider was not found; build the booth-provider target or set BOOTH_PROVIDER_BIN";
  }
  std::vector<std::string> args{binary.string()};
  const std::string value = trim(argument);
  if (value == "new") {
    args.push_back("--new");
  } else if (!value.empty()) {
    args.push_back("--provider");
    args.push_back(value);
  }
  std::vector<char*> raw;
  raw.reserve(args.size() + 1);
  for (auto& arg : args) raw.push_back(arg.data());
  raw.push_back(nullptr);

  int status = 0;
  int spawn_error = 0;
  screen.WithRestoredIO([&] {
    const pid_t child = ::fork();
    if (child == 0) {
      ::execv(binary.c_str(), raw.data());
      _exit(127);
    }
    if (child < 0) {
      spawn_error = errno;
      return;
    }
    while (::waitpid(child, &status, 0) < 0) {
      if (errno != EINTR) {
        spawn_error = errno;
        return;
      }
    }
  })();
  screen.PostEvent(ftxui::Event::Custom);
  if (spawn_error != 0) {
    return "cannot start booth-provider: " + std::string(std::strerror(spawn_error));
  }
  if (WIFEXITED(status) && WEXITSTATUS(status) == 0) return {};
  if (WIFEXITED(status)) {
    return "booth-provider exited with status " + std::to_string(WEXITSTATUS(status));
  }
  return "booth-provider was terminated";
}

}  // namespace booth
