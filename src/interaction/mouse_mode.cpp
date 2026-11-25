#include "interaction/mouse_mode.hpp"

#include <cstdlib>
#include <fstream>
#include <iostream>
#include <iterator>

#include <ftxui/component/screen_interactive.hpp>

#include "interaction/state.hpp"

namespace booth {
namespace {

std::string trim(std::string_view value) {
  const auto first = value.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = value.find_last_not_of(" \t\r\n");
  return std::string(value.substr(first, last - first + 1));
}

}  // namespace

bool ssh_session() {
  return ::getenv("SSH_CONNECTION") != nullptr || ::getenv("SSH_TTY") != nullptr ||
         ::getenv("SSH_CLIENT") != nullptr;
}

bool default_mouse_enabled() { return ::getenv("TMUX") == nullptr && !ssh_session(); }

bool load_mouse_enabled(const std::filesystem::path& state_dir) {
  std::ifstream input(state_dir / "mouse", std::ios::binary);
  if (!input) return default_mouse_enabled();
  const std::string value((std::istreambuf_iterator<char>(input)),
                          std::istreambuf_iterator<char>());
  const std::string cleaned = trim(value);
  if (cleaned == "off") return false;
  if (cleaned == "on") return true;
  return default_mouse_enabled();
}

bool save_mouse_enabled(const std::filesystem::path& state_dir, bool enabled) {
  std::error_code error;
  std::filesystem::create_directories(state_dir, error);
  if (error) return false;
  const auto path = state_dir / "mouse";
  const auto temporary = state_dir / "mouse.tmp";
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

std::string mouse_enable_sequence() {
  return "\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h\x1b[?1015h";
}

std::string mouse_disable_sequence() {
  return "\x1b[?1000l\x1b[?1002l\x1b[?1003l\x1b[?1006l\x1b[?1015l";
}

void apply_mouse_capture(ftxui::ScreenInteractive& screen, bool enabled) {
  screen.TrackMouse(enabled);
  std::cout << (enabled ? mouse_enable_sequence() : mouse_disable_sequence()) << std::flush;
}

bool handle_mouse_command(std::string_view line, ftxui::ScreenInteractive& screen, AppState& state,
                          const std::filesystem::path& state_dir) {
  const std::string value = trim(line);
  bool enabled = load_mouse_enabled(state_dir);
  if (value == "/mouse") {
    state.notice = enabled ? "mouse capture on" : "mouse capture off (terminal/tmux owns scroll and selection)";
    return enabled;
  }
  const std::string setting = trim(std::string_view(value).substr(std::string("/mouse").size()));
  if (setting != "on" && setting != "off") {
    state.notice = "usage: /mouse [on|off]";
    return enabled;
  }
  enabled = setting == "on";
  if (!save_mouse_enabled(state_dir, enabled)) {
    state.notice = "could not write mouse setting";
    return load_mouse_enabled(state_dir);
  }
  apply_mouse_capture(screen, enabled);
  state.notice = enabled ? "mouse capture on"
                         : "mouse capture off (terminal/tmux owns scroll and selection)";
  return enabled;
}

}  // namespace booth
