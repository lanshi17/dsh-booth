#include "interaction/history.hpp"

#include <nlohmann/json.hpp>

#include <fstream>
#include <system_error>

namespace booth {

std::vector<std::string> load_history(const std::filesystem::path& path) {
  std::vector<std::string> history;
  std::ifstream input(path);
  std::string line;
  while (std::getline(input, line)) {
    try {
      const auto value = nlohmann::json::parse(line);
      if (value.contains("text") && value["text"].is_string()) {
        history.push_back(value["text"].get<std::string>());
      }
    } catch (const std::exception&) {
      // One damaged line never hides the rest.
    }
  }
  if (history.size() > 200) history.erase(history.begin(), history.end() - 200);
  return history;
}

void append_history(const std::filesystem::path& path, const std::string& text) {
  std::ofstream output(path, std::ios::app);
  if (!output) return;
  output << nlohmann::json{{"text", text}}.dump() << '\n';
  output.close();
  std::error_code error;
  std::filesystem::permissions(path,
                               std::filesystem::perms::owner_read |
                                   std::filesystem::perms::owner_write,
                               std::filesystem::perm_options::replace, error);
}

}  // namespace booth
