#include "interaction/theme.hpp"

#include <cctype>
#include <cstdlib>
#include <fstream>
#include <string>

namespace booth {
namespace {

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

std::string upper(std::string_view text) {
  std::string out(text);
  for (auto& ch : out) ch = static_cast<char>(std::toupper(static_cast<unsigned char>(ch)));
  return out;
}

}  // namespace

std::optional<ftxui::Color> parse_color(std::string_view text) {
  const std::string cleaned = trim(text);
  if (cleaned.size() != 7 || cleaned.front() != '#') return std::nullopt;
  unsigned value = 0;
  for (std::size_t index = 1; index < cleaned.size(); ++index) {
    const char ch = cleaned[index];
    if (!std::isxdigit(static_cast<unsigned char>(ch))) return std::nullopt;
    value = value * 16 + static_cast<unsigned>(
                             std::isdigit(static_cast<unsigned char>(ch))
                                 ? ch - '0'
                                 : std::tolower(static_cast<unsigned char>(ch)) - 'a' + 10);
  }
  return ftxui::Color::RGB((value >> 16) & 0xFF, (value >> 8) & 0xFF, value & 0xFF);
}

std::map<std::string, std::string> load_theme(const std::filesystem::path& path) {
  std::map<std::string, std::string> theme;
  std::ifstream input(path);
  std::string line;
  while (std::getline(input, line)) {
    const auto trimmed = trim(line);
    if (trimmed.empty() || trimmed.front() == '#') continue;
    const auto equals = trimmed.find('=');
    if (equals == std::string::npos) continue;
    const std::string key = trim(std::string_view(trimmed).substr(0, equals));
    const std::string color = trim(std::string_view(trimmed).substr(equals + 1));
    if (!key.empty() && parse_color(color)) theme[key] = color;
  }
  return theme;
}

void apply_theme(const std::map<std::string, std::string>& theme) {
  for (const auto& [key, value] : theme) {
    ::setenv(("BOOTH_THEME_" + upper(key)).c_str(), value.c_str(), 1);
  }
}

ftxui::Color theme_color(std::string_view key, ftxui::Color fallback) {
  const std::string name = "BOOTH_THEME_" + upper(key);
  const char* value = ::getenv(name.c_str());
  if (value == nullptr) return fallback;
  const auto color = parse_color(value);
  return color ? *color : fallback;
}

}  // namespace booth
