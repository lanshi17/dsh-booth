#include "interaction/file_command.hpp"

#include <fstream>
#include <sstream>
#include <vector>

namespace booth {
namespace {

std::string trim(std::string_view text) {
  const auto first = text.find_first_not_of(" \t\r\n");
  if (first == std::string_view::npos) return {};
  const auto last = text.find_last_not_of(" \t\r\n");
  return std::string(text.substr(first, last - first + 1));
}

std::string first_line(std::string_view text, std::size_t limit) {
  const auto end = text.find('\n');
  std::string line = trim(text.substr(0, end));
  if (line.size() > limit) {
    line.resize(limit);
    line += "…";
  }
  return line;
}

std::vector<std::string> split_args(std::string_view text) {
  std::vector<std::string> args;
  std::istringstream input{std::string(text)};
  std::string word;
  while (input >> word) args.push_back(word);
  return args;
}

void replace_all(std::string& text, std::string_view needle, std::string_view value) {
  std::size_t at = 0;
  while ((at = text.find(needle, at)) != std::string::npos) {
    text.replace(at, needle.size(), value);
    at += value.size();
  }
}

}  // namespace

std::optional<FileCommand> read_file_command(const std::filesystem::path& path) {
  std::ifstream input(path);
  if (!input) return std::nullopt;
  std::string all((std::istreambuf_iterator<char>(input)), std::istreambuf_iterator<char>());
  std::string description;
  std::string body = all;
  if (body.rfind("---", 0) == 0) {
    const auto end = body.find("\n---", 3);
    if (end == std::string::npos) return std::nullopt;
    std::istringstream header(body.substr(3, end - 3));
    std::string line;
    while (std::getline(header, line)) {
      const auto colon = line.find(':');
      if (colon == std::string::npos) continue;
      const std::string key = trim(std::string_view(line).substr(0, colon));
      if (key != "description" && key != "argument-hint" && key != "argumentHint") continue;
      if (key == "description") description = trim(std::string_view(line).substr(colon + 1));
    }
    body.erase(0, end + 4);
  }
  body = trim(body);
  if (body.empty()) return std::nullopt;
  if (description.empty()) description = first_line(body, 60);
  FileCommand command{path.stem().string(), std::move(description), std::move(body)};
  if (command.name.empty() || command.name.front() == '.') return std::nullopt;
  return command;
}

std::string expand_file_command(const FileCommand& command, std::string_view arguments) {
  const std::vector<std::string> args = split_args(arguments);
  std::string text = command.body;
  bool used = false;
  for (std::size_t index = 0; index < 9; ++index) {
    const std::string needle = "$" + std::to_string(index + 1);
    if (text.find(needle) != std::string::npos) {
      replace_all(text, needle, index < args.size() ? args[index] : "");
      used = true;
    }
  }
  for (const std::string_view needle : {"$ARGUMENTS", "$@"}) {
    if (text.find(needle) != std::string::npos) {
      replace_all(text, needle, arguments);
      used = true;
    }
  }
  if (!used && !arguments.empty()) {
    if (!text.empty() && text.back() != '\n') text += '\n';
    text += arguments;
  }
  return text;
}

}  // namespace booth
