#include "interaction/recorder_play.hpp"

#include <chrono>
#include <fstream>
#include <string>
#include <thread>

#include <nlohmann/json.hpp>

namespace booth {
namespace {

void restore(std::ostream& out) {
  out << "\x1b[?25h\x1b[?1049l" << std::flush;
}

}  // namespace

int play_recording(const std::filesystem::path& path, double speed, std::ostream& out,
                   std::ostream& error) {
  std::ifstream file(path);
  if (!file.is_open()) {
    error << "booth: cannot open recording " << path << '\n';
    return 1;
  }
  out << "\x1b[?1049h\x1b[?25l";
  long long previous = 0;
  bool frames = false;
  std::string line;
  while (std::getline(file, line)) {
    nlohmann::json row;
    try {
      row = nlohmann::json::parse(line);
    } catch (const nlohmann::json::exception&) {
      restore(out);
      error << "booth: invalid recording row\n";
      return 1;
    }
    if (row.value("type", std::string{}) != "frame") continue;
    const long long at = row.value("t", 0LL);
    if (speed > 0 && at > previous) {
      const auto delay = std::chrono::milliseconds(static_cast<long long>((at - previous) / speed));
      std::this_thread::sleep_for(delay);
    }
    previous = at;
    out << "\x1b[H" << row.value("screen", std::string{}) << std::flush;
    frames = true;
  }
  restore(out);
  if (!frames) {
    error << "booth: recording has no frames\n";
    return 1;
  }
  return 0;
}

}  // namespace booth
