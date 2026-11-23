#include "interaction/theme.hpp"

#include <array>
#include <cctype>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iterator>
#include <string>

namespace booth {
namespace {

constexpr std::array<std::string_view, 13> kThemeKeys = {
    "accent", "text", "dim", "line", "panel", "border", "green",
    "yellow", "red", "git", "gray", "token", "hover"};

bool supported_key(std::string_view key) {
  for (const auto allowed : kThemeKeys) {
    if (allowed == key) return true;
  }
  return false;
}

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
    std::string color = trim(std::string_view(trimmed).substr(equals + 1));
    const auto comment = color.find_first_of(" \t");
    if (comment != std::string::npos) color.resize(comment);
    if (supported_key(key) && parse_color(color)) theme[key] = color;
  }
  return theme;
}

ThemeUpdateResult update_theme_override(const std::filesystem::path& path,
                                        std::string_view assignment) {
  const auto equals = assignment.find('=');
  if (equals == std::string_view::npos) {
    return {false, {}, {}, "expected key=#rrggbb"};
  }
  const std::string key = trim(assignment.substr(0, equals));
  const std::string color = trim(assignment.substr(equals + 1));
  if (!supported_key(key)) return {false, {}, {}, "unknown theme key: " + key};
  if (!parse_color(color)) return {false, {}, {}, "invalid theme color: " + color};

  std::ifstream input(path, std::ios::binary);
  if (!input && std::filesystem::exists(path)) {
    return {false, {}, {}, "could not read theme file"};
  }
  const std::string original((std::istreambuf_iterator<char>(input)),
                             std::istreambuf_iterator<char>());
  std::string updated = original;
  std::size_t matching_value = std::string::npos;
  std::size_t matching_end = std::string::npos;
  for (std::size_t start = 0; start < original.size();) {
    const auto newline = original.find('\n', start);
    const auto end = newline == std::string::npos ? original.size() : newline;
    std::string_view line(original.data() + start, end - start);
    if (!line.empty() && line.back() == '\r') line.remove_suffix(1);
    const auto clean = trim(line);
    if (!clean.empty() && clean.front() != '#') {
      const auto separator = line.find('=');
      if (separator != std::string_view::npos &&
          trim(line.substr(0, separator)) == key) {
        const auto offset = line.substr(separator + 1).find_first_not_of(" \t");
        if (offset != std::string_view::npos) {
          const auto value_start = start + separator + 1 + offset;
          auto value_end = original.find_first_of(" \t\r\n", value_start);
          if (value_end == std::string::npos) value_end = original.size();
          matching_value = value_start;
          matching_end = value_end;
        }
      }
    }
    if (newline == std::string::npos) break;
    start = newline + 1;
  }
  if (matching_value == std::string::npos) {
    if (!updated.empty() && updated.back() != '\n') updated += '\n';
    updated += key + "=" + color + "\n";
  } else {
    updated.replace(matching_value, matching_end - matching_value, color);
  }

  const auto temporary = std::filesystem::path(path.string() + ".tmp");
  {
    std::ofstream output(temporary, std::ios::binary | std::ios::trunc);
    if (!output) return {false, {}, {}, "could not write theme file"};
    output.write(updated.data(), static_cast<std::streamsize>(updated.size()));
    if (!output) {
      output.close();
      std::error_code ignored;
      std::filesystem::remove(temporary, ignored);
      return {false, {}, {}, "could not write theme file"};
    }
  }
  std::error_code error;
  std::filesystem::rename(temporary, path, error);
  if (error) {
    std::filesystem::remove(temporary, error);
    return {false, {}, {}, "could not replace theme file"};
  }
  return {true, key, color, {}};
}

void apply_theme(const std::map<std::string, std::string>& theme) {
  for (const auto key : kThemeKeys) {
    ::unsetenv(("BOOTH_THEME_" + upper(key)).c_str());
  }
  for (const auto& [key, value] : theme) {
    if (supported_key(key) && parse_color(value)) {
      ::setenv(("BOOTH_THEME_" + upper(key)).c_str(), value.c_str(), 1);
    }
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
