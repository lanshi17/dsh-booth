#include "interaction/recorder.hpp"

#include <chrono>
#include <filesystem>
#include <fstream>
#include <system_error>

#include <ftxui/dom/elements.hpp>
#include <ftxui/dom/node.hpp>
#include <ftxui/screen/screen.hpp>
#include <nlohmann/json.hpp>

namespace booth {
namespace {

using Clock = std::chrono::steady_clock;

struct Recorder {
  bool active = false;
  std::ofstream file;
  std::string path;
  std::size_t frames = 0;
  Clock::time_point started{};
};

Recorder& recorder() {
  static Recorder value;
  return value;
}

long long record_elapsed_ms() {
  const auto& state = recorder();
  return std::chrono::duration_cast<std::chrono::milliseconds>(Clock::now() - state.started).count();
}

void append_row(const nlohmann::json& row) {
  auto& state = recorder();
  if (!state.file.is_open()) return;
  state.file << row.dump() << '\n';
}

}  // namespace

bool start_recording(const std::filesystem::path& path, std::string& error) {
  auto& state = recorder();
  if (state.active) {
    error = "already recording";
    return false;
  }
  std::error_code ec;
  if (!path.parent_path().empty()) std::filesystem::create_directories(path.parent_path(), ec);
  if (ec) {
    error = ec.message();
    return false;
  }
  state.file.open(path, std::ios::out | std::ios::trunc | std::ios::binary);
  if (!state.file.is_open()) {
    error = "cannot open " + path.string();
    return false;
  }
  std::filesystem::permissions(path, std::filesystem::perms::owner_read | std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, ec);
  state.active = true;
  state.path = path.string();
  state.frames = 0;
  state.started = Clock::now();
  append_row(nlohmann::json{{"type", "start"}, {"t", 0}});
  return true;
}

void stop_recording() {
  auto& state = recorder();
  if (!state.active) return;
  append_row(nlohmann::json{{"type", "stop"}, {"t", record_elapsed_ms()}, {"frames", state.frames}});
  state.file.flush();
  state.file.close();
  state.active = false;
}

bool recording() {
  return recorder().active;
}

std::string recording_path() {
  return recorder().path;
}

std::size_t recorded_frames() {
  return recorder().frames;
}

void record_frame(const ftxui::Element& element) {
  auto& state = recorder();
  if (!state.active) return;
  ftxui::Element sized = element;
  auto screen = ftxui::Screen::Create(ftxui::Dimension::Full(), ftxui::Dimension::Fit(sized));
  ftxui::Render(screen, sized);
  append_row(nlohmann::json{{"type", "frame"},
                            {"t", record_elapsed_ms()},
                            {"w", screen.dimx()},
                            {"h", screen.dimy()},
                            {"screen", screen.ToString()}});
  ++state.frames;
}

}  // namespace booth
