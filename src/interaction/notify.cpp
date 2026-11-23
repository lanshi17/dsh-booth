#include "interaction/notify.hpp"

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <iterator>
#include <string>

namespace booth {
namespace {

std::string trim(std::string_view value) {
  const auto first = value.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = value.find_last_not_of(" \t\r\n");
  return std::string(value.substr(first, last - first + 1));
}

}  // namespace

bool parse_notifications_enabled(std::string_view value) {
  const std::string cleaned = trim(value);
  if (cleaned == "off") return false;
  return true;
}

bool notifications_enabled(const std::filesystem::path& state_dir) {
  std::ifstream input(state_dir / "notify", std::ios::binary);
  if (!input) return true;
  const std::string value((std::istreambuf_iterator<char>(input)),
                          std::istreambuf_iterator<char>());
  return parse_notifications_enabled(value);
}

bool persist_notifications_enabled(const std::filesystem::path& state_dir, bool enabled) {
  std::error_code error;
  std::filesystem::create_directories(state_dir, error);
  if (error) return false;

  const auto path = state_dir / "notify";
  const auto temporary = state_dir / "notify.tmp";
  {
    std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
    if (!output) return false;
    output << (enabled ? "on\n" : "off\n");
    output.flush();
    if (!output) {
      output.close();
      std::filesystem::remove(temporary, error);
      return false;
    }
  }
  std::filesystem::rename(temporary, path, error);
  if (error) {
    std::filesystem::remove(temporary, error);
    return false;
  }
  return true;
}

std::string notification_sequence(const std::string& title, const std::string& body,
                                  bool tmux) {
  const std::string osc = "\x1b]9;" + title + ": " + body + "\x07";
  const std::string payload = tmux ? "\x1bPtmux;\x1b" + osc + "\x1b\\" : osc;
  return payload + "\x07";
}

void send_notification(const std::string& title, const std::string& body) {
  std::cout << notification_sequence(title, body, ::getenv("TMUX") != nullptr) << std::flush;
}

}  // namespace booth
