#include "interaction/session_pins.hpp"

#include <fstream>
#include <iterator>

namespace booth {

std::set<std::string> load_session_pins(const std::filesystem::path& path) {
  std::set<std::string> pins;
  std::ifstream input(path);
  std::string line;
  while (std::getline(input, line)) {
    if (!line.empty()) pins.insert(line);
  }
  return pins;
}

bool save_session_pins(const std::filesystem::path& path, const std::set<std::string>& pins) {
  std::error_code error;
  std::filesystem::create_directories(path.parent_path(), error);
  if (error) return false;
  const auto temporary = path.string() + ".tmp";
  {
    std::ofstream output(temporary, std::ios::trunc);
    if (!output) return false;
    for (const auto& id : pins) output << id << '\n';
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

bool set_session_pin(const std::filesystem::path& path, const std::string& id, bool pinned) {
  auto pins = load_session_pins(path);
  if (pinned) pins.insert(id);
  else pins.erase(id);
  return save_session_pins(path, pins);
}

}  // namespace booth
